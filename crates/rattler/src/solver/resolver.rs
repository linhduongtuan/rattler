use crate::match_spec::ParseMatchSpecError;
use crate::repo_data::{LazyRepoData, OwnedLazyRepoData};
use crate::{ChannelConfig, MatchSpec, PackageRecord, Version};
use bit_vec::BitVec;
use itertools::Itertools;
use once_cell::unsync::OnceCell;
use pubgrub::error::PubGrubError;
use pubgrub::report::{DefaultStringReporter, Reporter};
use pubgrub::solver::{Dependencies, Requirement};
use pubgrub::type_aliases::DependencyConstraints;
use pubgrub::version_set::VersionSet;
use std::fmt::Write;
use std::rc::Rc;
use std::str::FromStr;
use std::{
    borrow::Borrow,
    cell::RefCell,
    cmp::Ordering,
    collections::HashMap,
    error::Error,
    fmt::{Debug, Display, Formatter},
};

const ROOT_NAME: &str = "__ROOT__";

/// A trait that provides the solver with packages
pub trait PackageRecordProvider {
    /// Returns all records for the package with the given name.
    fn records(&self, package: &str) -> Result<Vec<PackageRecord>, Box<dyn Error>>;
}

impl<'input, 'repo: 'input> PackageRecordProvider for &'repo LazyRepoData<'input> {
    fn records(&self, package: &str) -> Result<Vec<PackageRecord>, Box<dyn Error>> {
        self.packages
            .get(package)
            .map(IntoIterator::into_iter)
            .into_iter()
            .flatten()
            .map(|record| record.parse().map_err(Into::into))
            .collect()
    }
}

impl PackageRecordProvider for OwnedLazyRepoData {
    fn records(&self, package: &str) -> Result<Vec<PackageRecord>, Box<dyn Error>> {
        self.repo_data().records(package)
    }
}

/// A complete set of all versions and variants of a single package.
///
/// A package in Conda has one or more entries these entries. Each entry of a single package is
/// called a variant. The `PackageVariants` struct holds a set of all available variants for a
/// single package. Each variant is represented by a `PackageVariantId`.
///
/// `PackageVariants` also holds an strict ordering of its variants which is used by the solver to
/// select the "best" variant. See [`Index::variants_order`] lazily computes and caches this order.
///
/// The [`Index::dependencies`] method can be used to return the dependencies of each variant.
///
/// Its also possible to compute a subset of this instance that only "selects" the variants that
/// match a certain matchspec. This subset is represented by a `PackageVariantsSubset` and is
/// computed using the [`PackageVariants::subset_from_matchspec`] method.
struct PackageVariants {
    name: String,

    /// A list of all records and the corresponding source index.
    variants: Vec<(usize, PackageRecord)>,

    /// The order of variants when sorted according to resolver rules
    solver_order: OnceCell<Vec<usize>>,

    /// List of dependencies of a specific record
    dependencies: Vec<OnceCell<Vec<MatchSpec>>>,
}

impl PackageVariants {
    /// Computes a subset of variants that match a certain [`MatchSpec`].
    pub fn subset_from_matchspec(self: Rc<Self>, match_spec: &MatchSpec) -> PackageVariantsSubset {
        // Construct a bitset that includes
        let mut included = BitVec::from_elem(self.variants.len(), false);
        for (idx, (_, variant)) in self.variants.iter().enumerate() {
            if match_spec.matches(variant) {
                included.set(idx, true)
            }
        }

        if included.none() {
            PackageVariantsSubset::Empty
        } else if included.all() {
            PackageVariantsSubset::Full
        } else {
            PackageVariantsSubset::Discrete(PackageVariantBitset {
                version_set: self,
                included,
            })
        }
    }

    /// Returns the number of variants contained within the given `PackageVariantsSubset`.
    pub fn subset_size(&self, range: &PackageVariantsSubset) -> usize {
        match range {
            PackageVariantsSubset::Empty => 0,
            PackageVariantsSubset::Full => self.variants.len(),
            PackageVariantsSubset::Discrete(discrete) => discrete
                .included
                .blocks()
                .map(|b| b.count_ones())
                .sum::<u32>() as usize,
        }
    }
}

/// Represents a single variant within a `PackageVariants`.
#[derive(Clone)]
struct PackageVariantId {
    version_set: Rc<PackageVariants>,
    index: usize,
}

impl PackageVariantId {
    /// Returns the name of the package of the variant.
    pub fn name(&self) -> &str {
        &self.version_set.name
    }
}

impl Debug for PackageVariantId {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}#{}", self.name(), self.index)
    }
}

impl Display for PackageVariantId {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let (_, variant) = &self.version_set.variants[self.index];
        if variant.name == ROOT_NAME {
            write!(f, "the environment")
        } else {
            write!(f, "{}", variant)
        }
    }
}

impl PartialEq for PackageVariantId {
    fn eq(&self, other: &Self) -> bool {
        debug_assert!(Rc::ptr_eq(&self.version_set, &other.version_set));
        self.index == other.index
    }
}

impl Eq for PackageVariantId {}

impl PartialOrd for PackageVariantId {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        debug_assert!(Rc::ptr_eq(&self.version_set, &other.version_set));
        self.index.partial_cmp(&other.index)
    }
}

impl Ord for PackageVariantId {
    fn cmp(&self, other: &Self) -> Ordering {
        debug_assert!(Rc::ptr_eq(&self.version_set, &other.version_set));
        self.index.cmp(&other.index)
    }
}

#[derive(Clone)]
struct PackageVariantBitset {
    version_set: Rc<PackageVariants>,
    included: BitVec,
}

impl PackageVariantBitset {
    /// Returns true if the given variant is contained within this instance.
    #[inline]
    pub fn contains(&self, id: &PackageVariantId) -> bool {
        self.contains_index(id.index)
    }

    /// Returns true if the given variant index is contained within this instance.
    #[inline]
    pub fn contains_index(&self, index: usize) -> bool {
        self.included
            .get(index)
            .expect("could not sample outside of available package versions")
    }
}

impl Debug for PackageVariantBitset {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PackageVariantBitset")
            .field("version_set", &self.version_set.name)
            .field("included", &self.included)
            .finish()
    }
}

impl Display for PackageVariantBitset {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        // let versions = self
        //     .included
        //     .iter()
        //     .enumerate()
        //     .filter_map(|(index, selected)| {
        //         if selected {
        //             Some(self.version_set.variants[index].to_string())
        //         } else {
        //             None
        //         }
        //     })
        //     .join(", ");
        // write!(f, "{}", versions)
        write!(f, "?")
    }
}

impl PartialEq for PackageVariantBitset {
    fn eq(&self, other: &Self) -> bool {
        self.included.eq(&other.included)
    }
}

impl Eq for PackageVariantBitset {}

impl From<PackageVariantBitset> for PackageVariantsSubset {
    fn from(range: PackageVariantBitset) -> Self {
        PackageVariantsSubset::Discrete(range)
    }
}

impl PackageVariantBitset {
    pub fn singleton(v: PackageVariantId) -> Self {
        let mut included = BitVec::from_elem(v.version_set.variants.len(), false);
        included.set(v.index, true);
        PackageVariantBitset {
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

#[derive(Clone, Debug, Eq, PartialEq)]
enum PackageVariantsSubset {
    Empty,
    Full,
    Discrete(PackageVariantBitset),
}

impl Display for PackageVariantsSubset {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            PackageVariantsSubset::Empty => write!(f, "!"),
            PackageVariantsSubset::Full => write!(f, "*"),
            PackageVariantsSubset::Discrete(discrete) => write!(f, "{}", discrete),
        }
    }
}

impl PackageVariantsSubset {
    fn contains_index(&self, index: usize) -> bool {
        match self {
            PackageVariantsSubset::Empty => false,
            PackageVariantsSubset::Full => true,
            PackageVariantsSubset::Discrete(bitset) => bitset.contains_index(index),
        }
    }
}

impl VersionSet for PackageVariantsSubset {
    type V = PackageVariantId;

    fn empty() -> Self {
        Self::Empty
    }

    fn full() -> Self {
        Self::Full
    }

    fn singleton(v: Self::V) -> Self {
        PackageVariantBitset::singleton(v).into()
    }

    fn complement(&self) -> Self {
        match self {
            PackageVariantsSubset::Empty => PackageVariantsSubset::Full,
            PackageVariantsSubset::Full => PackageVariantsSubset::Empty,
            PackageVariantsSubset::Discrete(discrete) => {
                PackageVariantsSubset::Discrete(discrete.complement())
            }
        }
    }

    fn intersection(&self, other: &Self) -> Self {
        match (self, other) {
            (PackageVariantsSubset::Empty, _) | (_, PackageVariantsSubset::Empty) => {
                PackageVariantsSubset::Empty
            }
            (PackageVariantsSubset::Full, other) | (other, PackageVariantsSubset::Full) => {
                other.clone()
            }
            (PackageVariantsSubset::Discrete(a), PackageVariantsSubset::Discrete(b)) => {
                debug_assert!(Rc::ptr_eq(&a.version_set, &b.version_set));
                let mut included = a.included.clone();
                included.and(&b.included);
                if included.none() {
                    PackageVariantsSubset::Empty
                } else {
                    PackageVariantBitset {
                        version_set: a.version_set.clone(),
                        included,
                    }
                    .into()
                }
            }
        }
    }

    fn contains(&self, v: &Self::V) -> bool {
        match self {
            PackageVariantsSubset::Empty => false,
            PackageVariantsSubset::Full => true,
            PackageVariantsSubset::Discrete(bitset) => bitset.contains(v),
        }
    }

    fn union(&self, other: &Self) -> Self {
        match (self, other) {
            (PackageVariantsSubset::Empty, other) | (other, PackageVariantsSubset::Empty) => {
                other.clone()
            }
            (PackageVariantsSubset::Full, _) | (_, PackageVariantsSubset::Full) => {
                PackageVariantsSubset::Full
            }
            (PackageVariantsSubset::Discrete(a), PackageVariantsSubset::Discrete(b)) => {
                debug_assert!(Rc::ptr_eq(&a.version_set, &b.version_set));
                let mut included = a.included.clone();
                included.or(&b.included);
                if included.all() {
                    PackageVariantsSubset::Full
                } else {
                    PackageVariantBitset {
                        version_set: a.version_set.clone(),
                        included,
                    }
                    .into()
                }
            }
        }
    }
}

pub struct Index<C: Clone, P: PackageRecordProvider> {
    /// A cache of package variants
    package_variants_cache: RefCell<HashMap<String, Rc<PackageVariants>>>,

    /// A cache of highest versions for a given matchspec
    match_spec_cache: RefCell<HashMap<MatchSpec, (Version, bool)>>,

    /// Repodata used by the index
    repo_datas: Vec<(C, P)>,

    /// Channel configuration used by the index
    pub channel_config: ChannelConfig,
}

impl<C: Clone, P: PackageRecordProvider> Index<C, P> {
    /// Constructs a new index
    pub fn new(repos: impl IntoIterator<Item = (C, P)>, channel_config: ChannelConfig) -> Self {
        Self {
            package_variants_cache: RefCell::new(Default::default()),
            match_spec_cache: RefCell::new(Default::default()),
            repo_datas: repos.into_iter().collect(),
            channel_config,
        }
    }

    pub fn solve(
        &self,
        specs: impl IntoIterator<Item = MatchSpec>,
    ) -> Result<Vec<(C, PackageRecord)>, String> {
        let root_package_name = ROOT_NAME.to_owned();
        let root_version = Version::from_str("0").unwrap();

        // Create variants (just the one) for the root
        let root_package_variant_set = Rc::new(PackageVariants {
            name: root_package_name.clone(),
            solver_order: Default::default(),
            dependencies: vec![OnceCell::with_value(specs.into_iter().collect())],
            variants: vec![(
                0,
                PackageRecord::new(root_package_name, root_version, String::from("0"), 0),
            )],
        });

        // Insert the root package name, don't care about any previous existing version
        self.package_variants_cache.borrow_mut().insert(
            root_package_variant_set.name.clone(),
            root_package_variant_set.clone(),
        );

        // Construct a single version of the root package (the only one)
        let root_package_variant = PackageVariantId {
            version_set: root_package_variant_set.clone(),
            index: 0,
        };

        // Run the solver
        match pubgrub::solver::resolve(
            self,
            root_package_variant.name().to_owned(),
            root_package_variant,
        ) {
            Ok(solution) => Ok(solution
                .into_values()
                .filter(|variant_id| {
                    !Rc::ptr_eq(&variant_id.version_set, &root_package_variant_set)
                })
                .map(|variant_id| variant_id.version_set.variants[variant_id.index].clone())
                .filter_map(|(c, record)| {
                    (c > 0).then(|| (self.repo_datas[c - 1].0.clone(), record))
                })
                .collect()),
            Err(PubGrubError::NoSolution(mut derivation_tree)) => {
                derivation_tree.collapse_no_versions();
                let mut err = String::new();
                writeln!(
                    &mut err,
                    "{}",
                    DefaultStringReporter::report(&derivation_tree)
                )
                .unwrap();
                Err(err)
            }
            Err(err) => {
                let mut error_message = String::new();
                writeln!(&mut error_message, "{err:?}").unwrap();
                Err(error_message)
            }
        }
    }

    /// Adds a virtual package to the index
    pub fn add_virtual_package(&mut self, package: PackageRecord) {
        let set = Rc::new(PackageVariants {
            name: package.name.clone(),
            solver_order: Default::default(),
            dependencies: vec![Default::default()],
            variants: vec![(0, package)],
        });

        if let Some(previous_package) = self
            .package_variants_cache
            .borrow_mut()
            .insert(set.name.clone(), set.clone())
        {
            panic!("duplicate package entry for `{}`", previous_package.name);
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
            let variants: Vec<(usize, PackageRecord)> = self
                .repo_datas
                .iter()
                .enumerate()
                .map(|(index, (_, repodata))| {
                    repodata
                        .records(package)
                        .map(|records| records.into_iter().map(move |record| (index + 1, record)))
                })
                .collect::<Result<Vec<_>, _>>()?
                .into_iter()
                .flatten()
                .collect();

            let set = Rc::new(PackageVariants {
                name: package.clone(),
                solver_order: Default::default(),
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
        variants.solver_order.get_or_init(|| {
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
        let (_, a) = &variants.variants[a_idx];
        let (_, b) = &variants.variants[b_idx];

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
    fn dependencies<'v>(
        &self,
        variants: &'v PackageVariants,
        variant_idx: usize,
    ) -> Result<&'v Vec<MatchSpec>, ParseMatchSpecError> {
        variants.dependencies[variant_idx].get_or_try_init(|| {
            let (_, record) = &variants.variants[variant_idx];
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
            .filter(|&(_, record)| match_spec.matches(record));

        // Determine the highest version as well as the whether all matching records have tracked
        // features.
        let result: Option<(Version, bool)> = matching_records.fold(None, |init, (_, record)| {
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

impl<C: Clone, P: PackageRecordProvider>
    pubgrub::solver::DependencyProvider<String, PackageVariantsSubset> for Index<C, P>
{
    fn choose_package_version<T: Borrow<String>, U: Borrow<PackageVariantsSubset>>(
        &self,
        potential_packages: impl Iterator<Item = (T, U)>,
    ) -> Result<(T, Option<PackageVariantId>), Box<dyn Error>> {
        let mut min_dependency_count = usize::MAX;
        let mut min_package = None;
        let mut num_packages = 0;
        for (package, range) in potential_packages {
            num_packages += 1;
            let variants = self.package_variants(package.borrow())?;
            let count = variants.subset_size(range.borrow());
            if count < min_dependency_count && count > 0 {
                min_package = Some((package, variants, range));
                min_dependency_count = count;
            }
        }

        if let Some((package, variants, range)) = min_package {
            for &variant_idx in self.variants_order(&variants).iter() {
                if range.borrow().contains_index(variant_idx) {
                    return Ok((
                        package,
                        Some(PackageVariantId {
                            version_set: variants.clone(),
                            index: variant_idx,
                        }),
                    ));
                }
            }
        }

        dbg!(
            "could not select any packages",
            num_packages,
            min_dependency_count
        );
        // for (package, range) in potential_packages {
        //     dbg!(package.borrow());
        // let variants = self.package_variants(package.borrow())?;
        // for &variant_idx in self.variants_order(&variants).iter() {
        //     if range.borrow().contains_variant_index(variant_idx) {
        //         return Ok((
        //             package,
        //             Some(VariantId {
        //                 version_set: variants.clone(),
        //                 index: variant_idx,
        //             }),
        //         ));
        //     }
        // }
        // }

        Err(anyhow::anyhow!("no packages found that can be chosen").into())
    }

    fn get_dependencies(
        &self,
        package: &String,
        version: &PackageVariantId,
    ) -> Result<Dependencies<String, PackageVariantsSubset>, Box<dyn Error>> {
        debug_assert!(package == &version.version_set.name);
        let (_, record) = &version.version_set.variants[version.index];
        let dependencies = self.dependencies(&version.version_set, version.index)?;

        let mut result: DependencyConstraints<String, Requirement<PackageVariantsSubset>> =
            DependencyConstraints::default();

        for constraint in record.constrains.iter() {
            let match_spec = MatchSpec::from_str(constraint, &self.channel_config)?;
            let name = match_spec
                .name
                .as_ref()
                .expect("matchspec without package name");

            let version_set = self.package_variants(name)?;
            let range = version_set.subset_from_matchspec(&match_spec);

            result
                .entry(name.clone())
                .and_modify(|spec| match spec {
                    Requirement::Required(spec_range) | Requirement::Constrained(spec_range) => {
                        *spec_range = spec_range.intersection(&range)
                    }
                })
                .or_insert_with(|| Requirement::Constrained(range));
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
                    tracing::warn!(
                        "{} has invalid dependency: could not find package entry for '{name}'",
                        version
                    );
                    return Ok(Dependencies::Unknown);
                    // return Err(anyhow::anyhow!("no entries found for package `{}`", name).into());
                }
            }

            let range = version_set.subset_from_matchspec(match_spec);

            let requirement = result
                .entry(name.clone())
                .and_modify(|spec| {
                    *spec = Requirement::Required(match spec {
                        Requirement::Required(spec_range)
                        | Requirement::Constrained(spec_range) => spec_range.intersection(&range),
                    });
                })
                .or_insert_with(|| Requirement::Required(range));

            let range = match requirement {
                Requirement::Required(range) | Requirement::Constrained(range) => range,
            };

            if range == &PackageVariantsSubset::empty() {
                tracing::warn!("{} has invalid dependency: the version range doesnt match any package for '{name}'", version);
                return Ok(Dependencies::Unknown);
            }
        }

        Ok(Dependencies::Known(result))
    }
}

#[cfg(test)]
mod test {
    use crate::repo_data::OwnedLazyRepoData;
    use crate::solver::resolver::Index;
    use crate::{MatchSpec, Platform};
    use insta::assert_yaml_snapshot;
    use itertools::Itertools;
    use once_cell::sync::Lazy;
    use std::path::PathBuf;

    fn conda_forge_repo_data_path(arch: Platform) -> PathBuf {
        format!(
            "{}/resources/channels/conda-forge/{}/repodata.json",
            env!("CARGO_MANIFEST_DIR"),
            arch
        )
        .into()
    }

    fn conda_forge_repo_data_linux_64() -> &'static OwnedLazyRepoData {
        static LINUX64_REPODATA: Lazy<OwnedLazyRepoData> = Lazy::new(|| {
            OwnedLazyRepoData::from_file(conda_forge_repo_data_path(Platform::Linux64))
                .expect("failed to read linux-64 conda-forge repodata")
        });
        &*LINUX64_REPODATA
    }

    fn conda_forge_repo_data_noarch() -> &'static OwnedLazyRepoData {
        static NOARCH_REPODATA: Lazy<OwnedLazyRepoData> = Lazy::new(|| {
            OwnedLazyRepoData::from_file(conda_forge_repo_data_path(Platform::NoArch))
                .expect("failed to read noarch conda-forge repodata")
        });
        &*NOARCH_REPODATA
    }

    fn solve(specs: impl IntoIterator<Item = impl AsRef<str>>) -> Result<Vec<String>, String> {
        let channel_config = Default::default();

        // Parse the specs
        let specs: Vec<_> = specs
            .into_iter()
            .map(|spec| MatchSpec::from_str(spec.as_ref(), &channel_config).unwrap())
            .collect();

        // Create the index
        let index = Index::new(
            [
                (0, conda_forge_repo_data_linux_64().repo_data()),
                (1, conda_forge_repo_data_noarch().repo_data()),
            ],
            channel_config,
        );

        // Call the solver
        index.solve(specs).map(|result| {
            result
                .iter()
                .map(|(_, record)| record.to_string())
                .sorted()
                .collect()
        })
    }

    #[test]
    pub fn solve_python() {
        assert_yaml_snapshot!(solve(["python"]));
    }

    #[test]
    pub fn order_doesnt_matter() {
        assert_eq!(
            solve(["python", "jupyterlab"]),
            solve(["jupyterlab", "python"])
        )
    }
}
