use crate::repo_data::LazyRepoData;
use crate::{ChannelConfig, MatchSpec, PackageRecord, Version};
use bit_vec::BitVec;
use itertools::Itertools;
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
use std::cell::Cell;

/// A complete set of all versions and variants of a single package.
struct PackageVersionSet {
    name: String,
    variants: RefCell<Vec<PackageRecord>>,
    sorted: Cell<bool>,
}

impl PackageVersionSet {
    pub fn range_from_matchspec(self: Rc<Self>, match_spec: &MatchSpec) -> PackageVersionRange {
        debug_assert!(self.sorted.get());
        let variants = self.variants.borrow();
        let mut included = BitVec::from_elem(variants.len(), false);
        for (idx, variant) in variants.iter().enumerate() {
            if match_spec.matches(variant) {
                included.set(idx, true)
            }
        }
        drop(variants);

        if included.none() {
            PackageVersionRange::Empty
        } else if included.all() {
            PackageVersionRange::Full
        } else {
            PackageVersionRange::Discrete(DiscretePackageVersionRange {
                version_set: self,
                included,
            })
        }
    }
}

#[derive(Clone)]
struct PackageVersionId {
    version_set: Rc<PackageVersionSet>,
    index: usize,
}

impl Debug for PackageVersionId {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let variants = self.version_set.variants.borrow();
        write!(f, "{}", &variants[self.index])
    }
}

impl Display for PackageVersionId {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let variants = self.version_set.variants.borrow();
        write!(f, "{}", &variants[self.index])
    }
}

impl PartialEq for PackageVersionId {
    fn eq(&self, other: &Self) -> bool {
        debug_assert!(Rc::ptr_eq(&self.version_set, &other.version_set));
        self.index == other.index
    }
}
impl Eq for PackageVersionId {}

impl PartialOrd for PackageVersionId {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        debug_assert!(Rc::ptr_eq(&self.version_set, &other.version_set));
        self.index.partial_cmp(&other.index)
    }
}

impl Ord for PackageVersionId {
    fn cmp(&self, other: &Self) -> Ordering {
        debug_assert!(Rc::ptr_eq(&self.version_set, &other.version_set));
        self.index.cmp(&other.index)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum PackageVersionRange {
    Empty,
    Full,
    Discrete(DiscretePackageVersionRange),
}

impl Display for PackageVersionRange {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            PackageVersionRange::Empty => write!(f, "!"),
            PackageVersionRange::Full => write!(f, "*"),
            PackageVersionRange::Discrete(discrete) => write!(f, "{}", discrete),
        }
    }
}

#[derive(Clone)]
struct DiscretePackageVersionRange {
    version_set: Rc<PackageVersionSet>,
    included: BitVec,
}

impl Debug for DiscretePackageVersionRange {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DiscretePackageVersionRange")
            .field("version_set", &self.version_set.name)
            .field("included", &self.included)
            .finish()
    }
}

impl Display for DiscretePackageVersionRange {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let versions = self
            .included
            .iter()
            .enumerate()
            .filter_map(|(index, selected)| {
                if selected {
                    Some(self.version_set.variants.borrow()[index].to_string())
                } else {
                    None
                }
            })
            .join(", ");
        write!(f, "{}", versions)
    }
}

impl PartialEq for DiscretePackageVersionRange {
    fn eq(&self, other: &Self) -> bool {
        self.included.eq(&other.included)
    }
}

impl Eq for DiscretePackageVersionRange {}

impl From<DiscretePackageVersionRange> for PackageVersionRange {
    fn from(range: DiscretePackageVersionRange) -> Self {
        PackageVersionRange::Discrete(range)
    }
}

impl DiscretePackageVersionRange {
    pub fn singleton(v: PackageVersionId) -> Self {
        let mut included = BitVec::from_elem(v.version_set.variants.borrow().len(), false);
        included.set(v.index, true);
        DiscretePackageVersionRange {
            version_set: v.version_set,
            included,
        }
    }

    pub fn complement(&self) -> Self {
        let mut included = self.included.clone();
        included.negate();
        Self {
            version_set: self.version_set.clone(),
            included: included,
        }
    }
}

impl VersionSet for PackageVersionRange {
    type V = PackageVersionId;

    fn empty() -> Self {
        Self::Empty
    }

    fn full() -> Self {
        Self::Full
    }

    fn singleton(v: Self::V) -> Self {
        DiscretePackageVersionRange::singleton(v).into()
    }

    fn complement(&self) -> Self {
        match self {
            PackageVersionRange::Empty => PackageVersionRange::Full,
            PackageVersionRange::Full => PackageVersionRange::Empty,
            PackageVersionRange::Discrete(discrete) => {
                PackageVersionRange::Discrete(discrete.complement())
            }
        }
    }

    fn intersection(&self, other: &Self) -> Self {
        match (self, other) {
            (PackageVersionRange::Empty, _) | (_, PackageVersionRange::Empty) => {
                PackageVersionRange::Empty
            }
            (PackageVersionRange::Full, other) | (other, PackageVersionRange::Full) => {
                other.clone()
            }
            (PackageVersionRange::Discrete(a), PackageVersionRange::Discrete(b)) => {
                debug_assert!(Rc::ptr_eq(&a.version_set, &b.version_set));
                let mut included = a.included.clone();
                included.and(&b.included);
                if included.none() {
                    PackageVersionRange::Empty
                } else {
                    DiscretePackageVersionRange {
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
            PackageVersionRange::Empty => false,
            PackageVersionRange::Full => true,
            PackageVersionRange::Discrete(discrete) => discrete
                .included
                .get(v.index)
                .expect("could not sample outside of available package versions"),
        }
    }

    fn union(&self, other: &Self) -> Self {
        match (self, other) {
            (PackageVersionRange::Empty, other) | (other, PackageVersionRange::Empty) => {
                other.clone()
            }
            (PackageVersionRange::Full, _) | (_, PackageVersionRange::Full) => {
                PackageVersionRange::Full
            }
            (PackageVersionRange::Discrete(a), PackageVersionRange::Discrete(b)) => {
                debug_assert!(Rc::ptr_eq(&a.version_set, &b.version_set));
                let mut included = a.included.clone();
                included.or(&b.included);
                if included.all() {
                    PackageVersionRange::Full
                } else {
                    DiscretePackageVersionRange {
                        version_set: a.version_set.clone(),
                        included,
                    }
                    .into()
                }
            }
        }
    }
}

impl PackageVersionSet {
    fn available_versions(self: Rc<Self>) -> impl Iterator<Item = PackageVersionId> {
        let len = self.variants.borrow().len();
        (0..len).map(move |index| PackageVersionId {
            version_set: self.clone(),
            index,
        })
    }
}

struct Index<'i> {
    cached_dependencies: RefCell<HashMap<String, Rc<PackageVersionSet>>>,
    repo_datas: Vec<LazyRepoData<'i>>,
    channel_config: ChannelConfig,
}

impl<'i> Index<'i> {
    fn version_set(
        &self,
        package: &String,
        sorted: bool,
    ) -> Result<Rc<PackageVersionSet>, Box<dyn Error>> {
        let borrow = self.cached_dependencies.borrow();
        let version_set = if let Some(entry) = borrow.get(package) {
            let result = entry.clone();
            drop(borrow);
            result
        } else {
            drop(borrow);
            let variants = self
                .repo_datas
                .iter()
                .flat_map(|repo| repo.packages.iter())
                .filter_map(|(filename, record)| {
                    if filename.starts_with(&format!("{}-", package)) {
                        match record.as_parsed() {
                            Ok(record) if &record.name == package => Some(Ok(record)),
                            Err(e) => Some(Err(e)),
                            Ok(_) => None,
                        }
                    } else {
                        None
                    }
                })
                .map_ok(Clone::clone)
                .collect::<Result<Vec<_>, _>>()?;

            let set = Rc::new(PackageVersionSet {
                name: package.clone(),
                variants: RefCell::new(variants),
                sorted: Cell::new(false),
            });

            self.cached_dependencies
                .borrow_mut()
                .entry(package.clone())
                .or_insert(set)
                .clone()
        };

        if sorted && !version_set.sorted.get() {
            version_set.variants.borrow_mut().sort_by(|a, b| {
                let a_has_tracked_features = a.track_features.is_empty();
                let b_has_tracked_features = b.track_features.is_empty();
                b_has_tracked_features
                    .cmp(&a_has_tracked_features)
                    .then_with(|| b.version.cmp(&a.version))
                    .then_with(|| b.build_number.cmp(&a.build_number))
                    .then_with(|| {
                        let a_match_specs = a
                            .depends
                            .iter()
                            .filter_map(|spec| {
                                match MatchSpec::from_str(&spec, &self.channel_config) {
                                    Ok(spec) => {
                                        spec.name.clone().map(|name| (name, spec))
                                    }
                                    Err(_) => None,
                                }
                            })
                            .collect::<HashMap<_, _>>();
                        let b_match_specs = b
                            .depends
                            .iter()
                            .filter_map(|spec| {
                                match MatchSpec::from_str(&spec, &self.channel_config) {
                                    Ok(spec) => {
                                        spec.name.clone().map(|name| (name, spec))
                                    }
                                    Err(_) => None,
                                }
                            })
                            .collect::<HashMap<_, _>>();

                        let mut total_score = 0;
                        for (dependency, a_spec) in a_match_specs.iter() {
                            if let Some(b_spec) = b_match_specs.get(dependency) {
                                let highest_a = self.find_highest_version(a_spec);
                                let highest_b = self.find_highest_version(b_spec);
                                // dbg!(&a_spec.name, &highest_a, &highest_b);
                                let score = match (highest_a, highest_b) {
                                    (None, Some(_)) => 1,
                                    (Some(_), None) => -1,
                                    (
                                        Some((a_version, a_tracked_features)),
                                        Some((b_version, b_tracked_features)),
                                    ) => {
                                        if a_tracked_features != b_tracked_features {
                                            if a_tracked_features {
                                                100
                                            } else {
                                                -100
                                            }
                                        } else {
                                            if a_version > b_version {
                                                -1
                                            } else {
                                                1
                                            }
                                        }
                                    }
                                    _ => 0,
                                };
                                total_score += score;
                            }
                        }

                        total_score.cmp(&0)
                    }).then_with(|| b.timestamp.cmp(&a.timestamp))
            });
            version_set.sorted.set(true);
        }

        Ok(version_set.clone())
    }

    fn find_highest_version(&self, match_spec: &MatchSpec) -> Option<(Version, bool)> {
        let name = match_spec.name.as_ref()?;
        let version_set = self.version_set(&name, false).ok()?;
        let variants = version_set.variants.borrow();
        let matching_records = variants
            .iter()
            .filter(|&record| match_spec.matches(record));
        matching_records.fold(None, |init, record| {
            Some(init.map_or_else(
                || (record.version.clone(), !record.track_features.is_empty()),
                |(version, has_tracked_features)| {
                    (
                        version.max(record.version.clone()),
                        has_tracked_features && record.track_features.is_empty(),
                    )
                },
            ))
        })
    }

    fn available_versions(
        &self,
        package: &String,
    ) -> Result<impl Iterator<Item = PackageVersionId>, Box<dyn Error>> {
        Ok(self.version_set(package, true)?.available_versions())
    }
}

impl<'i> pubgrub::solver::DependencyProvider<String, PackageVersionRange> for Index<'i> {
    fn choose_package_version<T: Borrow<String>, U: Borrow<PackageVersionRange>>(
        &self,
        mut potential_packages: impl Iterator<Item = (T, U)>,
    ) -> Result<(T, Option<PackageVersionId>), Box<dyn Error>> {
        let (package, range) = potential_packages.next().unwrap();
        let version = self
            .available_versions(package.borrow())?
            .filter(|v| range.borrow().contains(v))
            .next();
        Ok((package, version))
    }

    fn get_dependencies(
        &self,
        package: &String,
        version: &PackageVersionId,
    ) -> Result<Dependencies<String, PackageVersionRange>, Box<dyn Error>> {
        debug_assert!(package == &version.version_set.name);
        let variants = version.version_set.variants.borrow();
        let record = &variants[version.index];
        let mut dependencies: DependencyConstraints<String, Requirement<PackageVersionRange>> =
            DependencyConstraints::default();

        for constraint in record.constrains.iter() {
            let match_spec = MatchSpec::from_str(constraint, &self.channel_config)?;
            let name = match_spec
                .name
                .as_ref()
                .expect("matchspec without package name");

            let version_set = self.version_set(name, true)?;
            let range = version_set.range_from_matchspec(&match_spec);

            dependencies
                .entry(name.clone())
                .and_modify(|spec| {
                    spec.range = spec.range.intersection(&range);
                })
                .or_insert_with(|| Requirement::from_constraint(range));
        }

        for dependency in record.depends.iter() {
            let match_spec = MatchSpec::from_str(dependency, &self.channel_config)?;
            let name = match_spec
                .name
                .as_ref()
                .expect("matchspec without package name");

            let version_set = self.version_set(name, true)?;
            if version_set.variants.borrow().is_empty() {
                return Err(anyhow::anyhow!("no entries found for package `{}`", name).into());
            }

            let range = version_set.range_from_matchspec(&match_spec);

            dependencies
                .entry(name.clone())
                .and_modify(|spec| {
                    spec.range = spec.range.intersection(&range);
                    spec.kind = RequirementKind::Required;
                })
                .or_insert_with(|| Requirement::from_dependency(range));
        }

        Ok(Dependencies::Known(dependencies))
    }
}

#[cfg(test)]
mod test {
    use std::cell::{Cell, RefCell};
    use crate::repo_data::LazyRepoData;
    use crate::solver::resolver::{Index, PackageVersionId, PackageVersionSet};
    use crate::{PackageRecord, Version};
    use pubgrub::error::PubGrubError;
    use pubgrub::report::{DefaultStringReporter, Reporter};
    use std::rc::Rc;
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
        let version_set = Rc::new(PackageVersionSet {
            name: root_package_name.clone(),
            sorted: Cell::new(true),
            variants: RefCell::new(vec![PackageRecord {
                name: root_package_name.clone(),
                version: Version::from_str("0").unwrap(),
                build: "".to_string(),
                build_number: 0,
                subdir: "".to_string(),
                filename: None,
                md5: None,
                sha256: None,
                arch: None,
                platform: None,
                depends: vec![String::from("ogre")],
                constrains: vec![],
                track_features: vec![],
                features: None,
                noarch: Default::default(),
                preferred_env: None,
                license: None,
                license_family: None,
                timestamp: None,
                date: None,
                size: None,
            }]),
        });

        let root_version = PackageVersionId {
            index: 0,
            version_set: version_set.clone(),
        };

        let index = Index {
            repo_datas: vec![noarch_repo_data, linux64_repo_data],
            cached_dependencies: Default::default(),
            channel_config: Default::default(),
        };

        index
            .cached_dependencies
            .borrow_mut()
            .insert(root_package_name.clone(), version_set);

        match pubgrub::solver::resolve(&index, root_package_name, root_version) {
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
