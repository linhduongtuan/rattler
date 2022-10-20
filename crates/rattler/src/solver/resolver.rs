use crate::match_spec::ParseMatchSpecError;
use crate::repo_data::LazyRepoData;
use crate::{ChannelConfig, MatchSpec, PackageRecord, Version};
use bit_vec::BitVec;
use itertools::Itertools;
use once_cell::unsync::OnceCell;
use pubgrub::solver::{Dependencies, Requirement, RequirementKind};
use pubgrub::type_aliases::DependencyConstraints;
use pubgrub::version_set::VersionSet;
use std::rc::Rc;
use std::{
    borrow::Borrow,
    cell::RefCell,
    cmp::Ordering,
    collections::HashMap,
    error::Error,
    fmt::{Debug, Display, Formatter},
};

/// A complete set of all versions and variants of a single package.
struct PackageVariants {
    name: String,

    /// A list of all records
    variants: Vec<PackageRecord>,

    /// The order of variants when sorted according to resolver rules
    by_order: OnceCell<Vec<usize>>,

    /// List of dependencies of a specific record
    dependencies: Vec<OnceCell<Vec<MatchSpec>>>,
}

impl PackageVariants {
    pub fn range_from_matchspec(self: Rc<Self>, match_spec: &MatchSpec) -> PackageVariantSet {
        // Construct a bitset that includes
        let mut included = BitVec::from_elem(self.variants.len(), false);
        for (idx, variant) in self.variants.iter().enumerate() {
            if match_spec.matches(variant) {
                included.set(idx, true)
            }
        }

        if included.none() {
            PackageVariantSet::Empty
        } else if included.all() {
            PackageVariantSet::Full
        } else {
            PackageVariantSet::Discrete(PackageVariantRange {
                version_set: self,
                included,
            })
        }
    }
}

#[derive(Clone)]
struct VariantId {
    version_set: Rc<PackageVariants>,
    index: usize,
}

impl VariantId {
    pub fn name(&self) -> &str {
        &self.version_set.name
    }
}

impl Debug for VariantId {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let variants = &self.version_set.variants;
        write!(f, "{}", &variants[self.index])
    }
}

impl Display for VariantId {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let variants = &&self.version_set.variants;
        write!(f, "{}", &variants[self.index])
    }
}

impl PartialEq for VariantId {
    fn eq(&self, other: &Self) -> bool {
        debug_assert!(Rc::ptr_eq(&self.version_set, &other.version_set));
        self.index == other.index
    }
}
impl Eq for VariantId {}

impl PartialOrd for VariantId {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        debug_assert!(Rc::ptr_eq(&self.version_set, &other.version_set));
        self.index.partial_cmp(&other.index)
    }
}

impl Ord for VariantId {
    fn cmp(&self, other: &Self) -> Ordering {
        debug_assert!(Rc::ptr_eq(&self.version_set, &other.version_set));
        self.index.cmp(&other.index)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum PackageVariantSet {
    Empty,
    Full,
    Discrete(PackageVariantRange),
}

impl Display for PackageVariantSet {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            PackageVariantSet::Empty => write!(f, "!"),
            PackageVariantSet::Full => write!(f, "*"),
            PackageVariantSet::Discrete(discrete) => write!(f, "{}", discrete),
        }
    }
}

#[derive(Clone)]
struct PackageVariantRange {
    version_set: Rc<PackageVariants>,
    included: BitVec,
}

impl PackageVariantRange {
    #[inline]
    pub fn contains_variant_index(&self, idx: usize) -> bool {
        self.included
            .get(idx)
            .expect("could not sample outside of available package versions")
    }
}

impl Debug for PackageVariantRange {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DiscretePackageVersionRange")
            .field("version_set", &self.version_set.name)
            .field("included", &self.included)
            .finish()
    }
}

impl Display for PackageVariantRange {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let versions = self
            .included
            .iter()
            .enumerate()
            .filter_map(|(index, selected)| {
                if selected {
                    Some(self.version_set.variants[index].to_string())
                } else {
                    None
                }
            })
            .join(", ");
        write!(f, "{}", versions)
    }
}

impl PartialEq for PackageVariantRange {
    fn eq(&self, other: &Self) -> bool {
        self.included.eq(&other.included)
    }
}

impl Eq for PackageVariantRange {}

impl From<PackageVariantRange> for PackageVariantSet {
    fn from(range: PackageVariantRange) -> Self {
        PackageVariantSet::Discrete(range)
    }
}

impl PackageVariantRange {
    pub fn singleton(v: VariantId) -> Self {
        let mut included = BitVec::from_elem(v.version_set.variants.len(), false);
        included.set(v.index, true);
        PackageVariantRange {
            version_set: v.version_set,
            included,
        }
    }

    pub fn complement(&self) -> Self {
        let mut included = self.included.clone();
        included.negate();
        Self {
            version_set: self.version_set.clone(),
            included,
        }
    }
}

impl PackageVariantSet {
    pub fn contains_variant_index(&self, idx: usize) -> bool {
        match self {
            PackageVariantSet::Empty => false,
            PackageVariantSet::Full => true,
            PackageVariantSet::Discrete(discrete) => discrete.contains_variant_index(idx),
        }
    }
}

impl VersionSet for PackageVariantSet {
    type V = VariantId;

    fn empty() -> Self {
        Self::Empty
    }

    fn full() -> Self {
        Self::Full
    }

    fn singleton(v: Self::V) -> Self {
        PackageVariantRange::singleton(v).into()
    }

    fn complement(&self) -> Self {
        match self {
            PackageVariantSet::Empty => PackageVariantSet::Full,
            PackageVariantSet::Full => PackageVariantSet::Empty,
            PackageVariantSet::Discrete(discrete) => {
                PackageVariantSet::Discrete(discrete.complement())
            }
        }
    }

    fn intersection(&self, other: &Self) -> Self {
        match (self, other) {
            (PackageVariantSet::Empty, _) | (_, PackageVariantSet::Empty) => {
                PackageVariantSet::Empty
            }
            (PackageVariantSet::Full, other) | (other, PackageVariantSet::Full) => other.clone(),
            (PackageVariantSet::Discrete(a), PackageVariantSet::Discrete(b)) => {
                debug_assert!(Rc::ptr_eq(&a.version_set, &b.version_set));
                let mut included = a.included.clone();
                included.and(&b.included);
                if included.none() {
                    PackageVariantSet::Empty
                } else {
                    PackageVariantRange {
                        version_set: a.version_set.clone(),
                        included,
                    }
                    .into()
                }
            }
        }
    }

    fn contains(&self, v: &Self::V) -> bool {
        self.contains_variant_index(v.index)
    }

    fn union(&self, other: &Self) -> Self {
        match (self, other) {
            (PackageVariantSet::Empty, other) | (other, PackageVariantSet::Empty) => other.clone(),
            (PackageVariantSet::Full, _) | (_, PackageVariantSet::Full) => PackageVariantSet::Full,
            (PackageVariantSet::Discrete(a), PackageVariantSet::Discrete(b)) => {
                debug_assert!(Rc::ptr_eq(&a.version_set, &b.version_set));
                let mut included = a.included.clone();
                included.or(&b.included);
                if included.all() {
                    PackageVariantSet::Full
                } else {
                    PackageVariantRange {
                        version_set: a.version_set.clone(),
                        included,
                    }
                    .into()
                }
            }
        }
    }
}

struct Index<'i> {
    /// A cache of package variants
    package_variants_cache: RefCell<HashMap<String, Rc<PackageVariants>>>,

    /// A cache of highest versions for a given matchspec
    match_spec_cache: RefCell<HashMap<MatchSpec, (Version, bool)>>,

    /// Repodata used by the index
    repo_datas: Vec<LazyRepoData<'i>>,

    /// Channel configuration used by the index
    channel_config: ChannelConfig,
}

impl<'i> Index<'i> {
    /// Adds a virtual package to the index
    pub fn add_package(&mut self, package: PackageRecord) -> VariantId {
        let set = Rc::new(PackageVariants {
            name: package.name.clone(),
            by_order: Default::default(),
            dependencies: vec![Default::default()],
            variants: vec![package],
        });

        if let Some(previous_package) = self
            .package_variants_cache
            .borrow_mut()
            .insert(set.name.clone(), set.clone())
        {
            panic!("duplicate package entry for `{}`", previous_package.name);
        }

        VariantId {
            version_set: set,
            index: 0,
        }
    }

    /// Returns information about all the variants of a specific package.
    fn package_variants(&self, package: &String) -> Result<Rc<PackageVariants>, Box<dyn Error>> {
        let borrow = self.package_variants_cache.borrow();
        let version_set = if let Some(entry) = borrow.get(package) {
            let result = entry.clone();
            drop(borrow);
            result
        } else {
            drop(borrow);
            let variants = self
                .repo_datas
                .iter()
                .flat_map(|repodata| {
                    repodata
                        .packages
                        .get(package)
                        .map(IntoIterator::into_iter)
                        .into_iter()
                        .flatten()
                        .map(|record| record.parse())
                })
                .collect::<Result<Vec<_>, _>>()?;

            let set = Rc::new(PackageVariants {
                name: package.clone(),
                by_order: Default::default(),
                dependencies: (0..variants.len()).map(|_| Default::default()).collect(),
                variants,
            });

            self.package_variants_cache
                .borrow_mut()
                .entry(package.clone())
                .or_insert(set)
                .clone()
        };

        Ok(version_set)
    }

    /// Returns a vec that indicates the order of package variants.
    fn variants_order<'v>(&self, variants: &'v PackageVariants) -> &'v Vec<usize> {
        variants.by_order.get_or_init(|| {
            let mut result = (0..variants.variants.len()).collect_vec();
            result.sort_by(|&a, &b| self.compare_variants(variants, a, b));

            // eprintln!("-- {}", &variants.name);
            // for idx in result.iter() {
            //     let record = &variants.variants[*idx];
            //     eprintln!("  {}={}={}", &record.name, &record.version, &record.build);
            // }

            result
        })
    }

    /// Returns the order of two package variants based on rules used by conda.
    fn compare_variants(&self, variants: &PackageVariants, a_idx: usize, b_idx: usize) -> Ordering {
        let a = &variants.variants[a_idx];
        let b = &variants.variants[b_idx];

        // First compare by "tracked_features". If one of the packages has a tracked feature it is
        // sorted below the one that doesnt have the tracked feature.
        let a_has_tracked_features = a.track_features.is_empty();
        let b_has_tracked_features = b.track_features.is_empty();
        match b_has_tracked_features.cmp(&a_has_tracked_features) {
            Ordering::Less => return Ordering::Less,
            Ordering::Greater => return Ordering::Greater,
            Ordering::Equal => {}
        };

        // Otherwise, select the variant with the highest version
        match a.version.cmp(&b.version) {
            Ordering::Less => return Ordering::Greater,
            Ordering::Greater => return Ordering::Less,
            Ordering::Equal => {}
        };

        // Otherwise, select the variant with the highest build number
        match a.build_number.cmp(&b.build_number) {
            Ordering::Less => return Ordering::Greater,
            Ordering::Greater => return Ordering::Less,
            Ordering::Equal => {}
        };

        // Otherwise, compare the dependencies of the variants. If there are similar
        // dependencies select the variant that selects the highest version of the dependency.
        let empty_vec = Vec::new();
        let a_match_specs = self.dependencies(variants, a_idx).unwrap_or(&empty_vec);
        let b_match_specs = self.dependencies(variants, b_idx).unwrap_or(&empty_vec);

        let b_specs_by_name: HashMap<_, _> = b_match_specs
            .iter()
            .filter_map(|spec| spec.name.as_ref().map(|name| (name, spec)))
            .collect();

        let a_specs_by_name = a_match_specs
            .iter()
            .filter_map(|spec| spec.name.as_ref().map(|name| (name, spec)));

        let mut total_score = 0;
        for (a_dep_name, a_spec) in a_specs_by_name {
            if let Some(b_spec) = b_specs_by_name.get(&a_dep_name) {
                if &a_spec == b_spec {
                    continue;
                }

                // Find which of the two specs selects the highest version
                let highest_a = self.find_highest_version(a_spec);
                let highest_b = self.find_highest_version(b_spec);

                // Skip version if no package is selected by either spec
                let (a_version, a_tracked_features, b_version, b_tracked_features) = if let (
                    Some((a_version, a_tracked_features)),
                    Some((b_version, b_tracked_features)),
                ) =
                    (highest_a, highest_b)
                {
                    (a_version, a_tracked_features, b_version, b_tracked_features)
                } else {
                    continue;
                };

                // If one of the dependencies only selects versions with tracked features, down-
                // weight that variant.
                if let Some(score) = match a_tracked_features.cmp(&b_tracked_features) {
                    Ordering::Less => Some(-100),
                    Ordering::Greater => Some(100),
                    Ordering::Equal => None,
                } {
                    total_score += score;
                    continue;
                }

                // Otherwise, down-weigh the version with the lowest selected version.
                total_score += match a_version.cmp(&b_version) {
                    Ordering::Less => 1,
                    Ordering::Equal => 0,
                    Ordering::Greater => -1,
                };
            }
        }

        // If ranking the dependencies provides a score, use that for the sorting.
        match total_score.cmp(&0) {
            Ordering::Equal => {}
            ord => return ord,
        };

        // Otherwise, order by timestamp
        b.timestamp.cmp(&a.timestamp)
    }

    /// Returns the dependencies of a specific record of the given `PackageVariants`.
    pub fn dependencies<'v>(
        &self,
        variants: &'v PackageVariants,
        variant_idx: usize,
    ) -> Result<&'v Vec<MatchSpec>, ParseMatchSpecError> {
        variants.dependencies[variant_idx].get_or_try_init(|| {
            let record = &variants.variants[variant_idx];
            record
                .depends
                .iter()
                .map(|dep_str| MatchSpec::from_str(dep_str, &self.channel_config))
                .collect()
        })
    }

    // Given a spec determine the highest available version.
    fn find_highest_version(&self, match_spec: &MatchSpec) -> Option<(Version, bool)> {
        // First try to read from cache
        let borrow = self.match_spec_cache.borrow();
        if let Some(result) = borrow.get(match_spec) {
            return Some(result.clone());
        }
        drop(borrow);

        let name = match_spec.name.as_ref()?;

        // Get all records for the given package
        let version_set = self.package_variants(name).ok()?;

        // Create an iterator over all records that match
        let matching_records = version_set
            .variants
            .iter()
            .filter(|&record| match_spec.matches(record));

        // Determine the highest version as well as the whether all matching records have tracked
        // features.
        let result: Option<(Version, bool)> = matching_records.fold(None, |init, record| {
            Some(init.map_or_else(
                || (record.version.clone(), !record.track_features.is_empty()),
                |(version, has_tracked_features)| {
                    (
                        version.max(record.version.clone()),
                        has_tracked_features && record.track_features.is_empty(),
                    )
                },
            ))
        });

        // Store in cache for later
        if let Some(result) = &result {
            self.match_spec_cache
                .borrow_mut()
                .insert(match_spec.clone(), result.clone());
        }

        result
    }
}

impl<'i> pubgrub::solver::DependencyProvider<String, PackageVariantSet> for Index<'i> {
    fn choose_package_version<T: Borrow<String>, U: Borrow<PackageVariantSet>>(
        &self,
        potential_packages: impl Iterator<Item = (T, U)>,
    ) -> Result<(T, Option<VariantId>), Box<dyn Error>> {
        for (package, range) in potential_packages {
            let variants = self.package_variants(package.borrow())?;
            for &variant_idx in self.variants_order(&variants).iter() {
                if range.borrow().contains_variant_index(variant_idx) {
                    return Ok((
                        package,
                        Some(VariantId {
                            version_set: variants.clone(),
                            index: variant_idx,
                        }),
                    ));
                }
            }
        }

        Err(anyhow::anyhow!("no packages found that can be chosen").into())
    }

    fn get_dependencies(
        &self,
        package: &String,
        version: &VariantId,
    ) -> Result<Dependencies<String, PackageVariantSet>, Box<dyn Error>> {
        debug_assert!(package == &version.version_set.name);
        let record = &version.version_set.variants[version.index];
        let dependencies = self.dependencies(&version.version_set, version.index)?;

        let mut result: DependencyConstraints<String, Requirement<PackageVariantSet>> =
            DependencyConstraints::default();

        for constraint in record.constrains.iter() {
            let match_spec = MatchSpec::from_str(constraint, &self.channel_config)?;
            let name = match_spec
                .name
                .as_ref()
                .expect("matchspec without package name");

            let version_set = self.package_variants(name)?;
            let range = version_set.range_from_matchspec(&match_spec);

            result
                .entry(name.clone())
                .and_modify(|spec| {
                    spec.range = spec.range.intersection(&range);
                })
                .or_insert_with(|| Requirement::from_constraint(range));
        }

        for match_spec in dependencies {
            let name = match_spec
                .name
                .as_ref()
                .expect("matchspec without package name");

            let version_set = self.package_variants(name)?;
            if version_set.variants.is_empty() {
                if version_set.name.starts_with("__") {
                    return Ok(Dependencies::Unknown);
                } else {
                    return Err(anyhow::anyhow!("no entries found for package `{}`", name).into());
                }
            }

            let range = version_set.range_from_matchspec(match_spec);

            result
                .entry(name.clone())
                .and_modify(|spec| {
                    spec.range = spec.range.intersection(&range);
                    spec.kind = RequirementKind::Required;
                })
                .or_insert_with(|| Requirement::from_dependency(range));
        }

        Ok(Dependencies::Known(result))
    }
}

#[cfg(test)]
mod test {
    use crate::repo_data::LazyRepoData;
    use crate::solver::resolver::Index;
    use crate::{PackageRecord, Version};
    use pubgrub::error::PubGrubError;
    use pubgrub::report::{DefaultStringReporter, Reporter};
    use std::str::FromStr;

    fn conda_json_path() -> String {
        format!(
            "{}/{}",
            env!("CARGO_MANIFEST_DIR"),
            "resources/channels/conda-forge/linux-64/repodata.json"
        )
    }

    fn conda_json_path_noarch() -> String {
        format!(
            "{}/{}",
            env!("CARGO_MANIFEST_DIR"),
            "resources/channels/conda-forge/noarch/repodata.json"
        )
    }

    #[test]
    pub fn resolve_python() {
        let linux64_repo_data_str = std::fs::read_to_string(conda_json_path()).unwrap();
        let noarch_repo_data_str = std::fs::read_to_string(conda_json_path_noarch()).unwrap();

        let linux64_repo_data: LazyRepoData = serde_json::from_str(&linux64_repo_data_str).unwrap();
        let noarch_repo_data: LazyRepoData = serde_json::from_str(&noarch_repo_data_str).unwrap();

        let root_package_name = String::from("__ROOT__");
        let root_version = Version::from_str("0").unwrap();

        let mut index = Index {
            repo_datas: vec![noarch_repo_data, linux64_repo_data],
            package_variants_cache: Default::default(),
            channel_config: Default::default(),
            match_spec_cache: Default::default(),
        };

        let root_package_variant = index.add_package(PackageRecord {
            depends: vec![String::from("ogre")],
            ..PackageRecord::new(
                root_package_name.clone(),
                root_version.clone(),
                String::from(""),
                0,
            )
        });

        index.add_package(PackageRecord::new(
            String::from("__linux"),
            Version::from_str("5.10.102.1").unwrap(),
            String::from("0"),
            0,
        ));

        index.add_package(PackageRecord::new(
            String::from("__glibc"),
            Version::from_str("2.31").unwrap(),
            String::from("0"),
            0,
        ));

        index.add_package(PackageRecord::new(
            String::from("__unix"),
            Version::from_str("0").unwrap(),
            String::from("0"),
            0,
        ));

        index.add_package(PackageRecord::new(
            String::from("__archspec"),
            Version::from_str("1").unwrap(),
            String::from("x86_64"),
            0,
        ));

        match pubgrub::solver::resolve(
            &index,
            root_package_variant.name().to_owned(),
            root_package_variant,
        ) {
            Ok(solution) => println!("{:#?}", solution),
            Err(PubGrubError::NoSolution(mut derivation_tree)) => {
                derivation_tree.collapse_no_versions();
                eprintln!("{}", DefaultStringReporter::report(&derivation_tree));
            }
            Err(err) => panic!("{:?}", err),
        };

        panic!("err");

        // "__ROOT__": __ROOT__=0=,
        // "_libgcc_mutex": _libgcc_mutex=0.1=conda_forge,
        // "_openmp_mutex": _openmp_mutex=4.5=2_gnu,
        // "bzip2": bzip2=1.0.8=h7f98852_4,
        // "c-ares": c-ares=1.18.1=h7f98852_0,
        // "ca-certificates": ca-certificates=2022.9.24=ha878542_0,
        // "cmake": cmake=3.24.2=h5432695_0,
        // "expat": expat=2.4.9=h27087fc_0,
        // "keyutils": keyutils=1.6.1=h166bdaf_0,
        // "krb5": krb5=1.19.3=h08a2579_0,
        // "libcurl": libcurl=7.85.0=h2283fc2_0,
        // "libedit": libedit=3.1.20191231=he28a2e2_2,
        // "libev": libev=4.33=h516909a_1,
        // "libgcc-ng": libgcc-ng=12.2.0=h65d4601_18,
        // "libgomp": libgomp=12.2.0=h65d4601_18,
        // "libnghttp2": libnghttp2=1.47.0=hff17c54_1,
        // "libssh2": libssh2=1.10.0=hf14f497_3,
        // "libstdcxx-ng": libstdcxx-ng=12.2.0=h46fd767_18,
        // "libuv": libuv=1.44.2=h166bdaf_0,
        // "libzlib": libzlib=1.2.13=h166bdaf_4,
        // "ncurses": ncurses=6.3=h27087fc_1,
        // "openssl": openssl=3.0.5=h166bdaf_2,
        // "rhash": rhash=1.4.3=h166bdaf_0,
        // "xz": xz=5.2.6=h166bdaf_0,
        // "zlib": zlib=1.2.13=h166bdaf_4,
        // "zstd": zstd=1.5.2=h6239696_4,
    }
}
