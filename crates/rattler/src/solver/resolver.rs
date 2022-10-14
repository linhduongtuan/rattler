use crate::repo_data::LazyRepoData;
use crate::{ChannelConfig, MatchSpec, PackageRecord};
use bit_vec::BitVec;
use itertools::Itertools;
use pubgrub::solver::Dependencies;
use pubgrub::type_aliases::DependencyConstraints;
use pubgrub::version_set::VersionSet;
use std::{
    borrow::Borrow,
    cell::RefCell,
    cmp::Ordering,
    collections::HashMap,
    error::Error,
    fmt::{Debug, Display, Formatter},
    sync::Arc,
};

/// A complete set of all versions and variants of a single package.
struct PackageVersionSet {
    name: String,
    variants: Vec<PackageRecord>,
}

impl PackageVersionSet {
    pub fn range_from_matchspec(self: Arc<Self>, match_spec: &MatchSpec) -> PackageVersionRange {
        let mut included = BitVec::from_elem(self.variants.len(), false);
        for (idx, variant) in self.variants.iter().enumerate() {
            if match_spec.matches(variant) {
                included.set(idx, true)
            }
        }
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
    version_set: Arc<PackageVersionSet>,
    index: usize,
}

impl Debug for PackageVersionId {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", &self.version_set.variants[self.index])
    }
}

impl Display for PackageVersionId {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", &self.version_set.variants[self.index])
    }
}

impl PartialEq for PackageVersionId {
    fn eq(&self, other: &Self) -> bool {
        debug_assert!(Arc::ptr_eq(&self.version_set, &other.version_set));
        self.index == other.index
    }
}
impl Eq for PackageVersionId {}

impl PartialOrd for PackageVersionId {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        debug_assert!(Arc::ptr_eq(&self.version_set, &other.version_set));
        self.index.partial_cmp(&other.index)
    }
}

impl Ord for PackageVersionId {
    fn cmp(&self, other: &Self) -> Ordering {
        debug_assert!(Arc::ptr_eq(&self.version_set, &other.version_set));
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
    version_set: Arc<PackageVersionSet>,
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
                    Some(self.version_set.variants[index].to_string())
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
        let mut included = BitVec::from_elem(v.version_set.variants.len(), false);
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

impl pubgrub::version_set::VersionSet for PackageVersionRange {
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
                debug_assert!(Arc::ptr_eq(&a.version_set, &b.version_set));
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
                debug_assert!(Arc::ptr_eq(&a.version_set, &b.version_set));
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
    fn available_versions(self: Arc<Self>) -> impl Iterator<Item = PackageVersionId> {
        (0..self.variants.len()).map(move |index| PackageVersionId {
            version_set: self.clone(),
            index,
        })
    }
}

struct Index<'i> {
    cached_dependencies: RefCell<HashMap<String, Arc<PackageVersionSet>>>,
    repo_datas: Vec<LazyRepoData<'i>>,
    channel_config: ChannelConfig,
}

impl<'i> Index<'i> {
    fn version_set(&self, package: &String) -> Result<Arc<PackageVersionSet>, Box<dyn Error>> {
        let borrow = self.cached_dependencies.borrow();
        Ok(if let Some(entry) = borrow.get(package) {
            entry.clone()
        } else {
            drop(borrow);
            let mut variants = self
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

            variants.sort();
            variants.reverse();

            if variants.is_empty() {
                return Err(anyhow::anyhow!("No package entries found for '{package}'").into());
            }

            let set = Arc::new(PackageVersionSet {
                name: package.clone(),
                variants,
            });
            self.cached_dependencies
                .borrow_mut()
                .insert(package.clone(), set.clone());
            set
        })
    }

    fn available_versions(
        &self,
        package: &String,
    ) -> Result<impl Iterator<Item = PackageVersionId>, Box<dyn Error>> {
        Ok(self.version_set(package)?.available_versions())
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
        let record = &version.version_set.variants[version.index];
        let mut dependencies: DependencyConstraints<String, PackageVersionRange> =
            DependencyConstraints::default();
        for dependency in record.depends.iter() {
            dbg!(dependency);
            let match_spec = MatchSpec::from_str(dependency, &self.channel_config)?;
            let name = match_spec
                .name
                .as_ref()
                .expect("matchspec without package name");
            let version_set = self.version_set(name)?;

            let range = version_set.range_from_matchspec(&match_spec);

            dependencies
                .entry(name.clone())
                .and_modify(|spec| {
                    *spec = spec.intersection(&range);
                })
                .or_insert(range);
        }

        Ok(Dependencies::Known(dependencies))
    }
}

#[cfg(test)]
mod test {
    use crate::repo_data::LazyRepoData;
    use crate::solver::resolver::{Index, PackageVersionId, PackageVersionSet};
    use crate::{PackageRecord, Version};
    use pubgrub::error::PubGrubError;
    use pubgrub::report::{DefaultStringReporter, Reporter};
    use std::str::FromStr;
    use std::sync::Arc;

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
        let version_set = Arc::new(PackageVersionSet {
            name: root_package_name.clone(),
            variants: vec![PackageRecord {
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
                depends: vec![String::from("python =3.9")],
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
            }],
        });

        let root_version = PackageVersionId {
            index: 0,
            version_set: version_set.clone(),
        };

        let index = Index {
            repo_datas: vec![linux64_repo_data, noarch_repo_data],
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
        // "_openmp_mutex": _openmp_mutex=4.5=2_kmp_llvm,
        // "bzip2": bzip2=1.0.8=h7f98852_4,
        // "ca-certificates": ca-certificates=2022.6.15=ha878542_0,
        // "ld_impl_linux-64": ld_impl_linux-64=2.36.1=hea4e1c9_2,
        // "libffi": libffi=3.4.2=h9c3ff4c_4,
        // "libgcc-ng": libgcc-ng=12.1.0=h8d9b700_16,
        // "libnsl": libnsl=2.0.0=h7f98852_0,
        // "libsqlite": libsqlite=3.39.2=h753d276_1,
        // "libstdcxx-ng": libstdcxx-ng=12.1.0=ha89aaad_16,
        // "libuuid": libuuid=2.32.1=h7f98852_1000,
        // "libzlib": libzlib=1.2.12=h166bdaf_2,
        // "llvm-openmp": llvm-openmp=14.0.4=he0ac6c6_0,
        // "ncurses": ncurses=6.3=h9c3ff4c_0,
        // "openssl": openssl=1.1.1q=h166bdaf_0,
        // "python": python=3.9.13=h9a8a25e_0_cpython,
        // "readline": readline=8.1.2=h0f457ee_0,
        // "sqlite": sqlite=3.39.2=h4ff8645_1,
        // "tk": tk=8.6.12=h27826a3_0,
        // "tzdata": tzdata=2021e=he74cb21_0,
        // "xz": xz=5.2.6=h166bdaf_0,

        // Install - SolvableInfo { name: "_libgcc_mutex", version: "0.1", build_string: Some("conda_forge"), build_number: Some(0) }
        // Install - SolvableInfo { name: "_openmp_mutex", version: "4.5", build_string: Some("1_llvm"), build_number: Some(1) }
        // Install - SolvableInfo { name: "bzip2", version: "1.0.8", build_string: Some("h7f98852_4"), build_number: Some(4) }
        // Install - SolvableInfo { name: "ca-certificates", version: "2022.6.15", build_string: Some("ha878542_0"), build_number: Some(0) }
        // Install - SolvableInfo { name: "ld_impl_linux-64", version: "2.36.1", build_string: Some("hea4e1c9_0"), build_number: Some(0) }
        // Install - SolvableInfo { name: "libffi", version: "3.4.2", build_string: Some("h9c3ff4c_2"), build_number: Some(2) }
        // Install - SolvableInfo { name: "libgcc-ng", version: "12.1.0", build_string: Some("h8d9b700_16"), build_number: Some(16) }
        // Install - SolvableInfo { name: "libnsl", version: "2.0.0", build_string: Some("h7f98852_0"), build_number: Some(0) }
        // Install - SolvableInfo { name: "libstdcxx-ng", version: "12.1.0", build_string: Some("ha89aaad_16"), build_number: Some(16) }
        // Install - SolvableInfo { name: "libuuid", version: "2.32.1", build_string: Some("h7f98852_1000"), build_number: Some(1000) }
        // Install - SolvableInfo { name: "libzlib", version: "1.2.12", build_string: Some("h166bdaf_1"), build_number: Some(1) }
        // Install - SolvableInfo { name: "llvm-openmp", version: "14.0.4", build_string: Some("he0ac6c6_0"), build_number: Some(0) }
        // Install - SolvableInfo { name: "ncurses", version: "6.3", build_string: Some("h9c3ff4c_0"), build_number: Some(0) }
        // Install - SolvableInfo { name: "openssl", version: "1.1.1q", build_string: Some("h166bdaf_0"), build_number: Some(0) }
        // Install - SolvableInfo { name: "python", version: "3.9.13", build_string: Some("h9a8a25e_0_cpython"), build_number: Some(0) }
        // Install - SolvableInfo { name: "readline", version: "8.1.2", build_string: Some("h0f457ee_0"), build_number: Some(0) }
        // Install - SolvableInfo { name: "sqlite", version: "3.39.2", build_string: Some("h4ff8645_0"), build_number: Some(0) }
        // Install - SolvableInfo { name: "tk", version: "8.6.12", build_string: Some("h27826a3_0"), build_number: Some(0) }
        // Install - SolvableInfo { name: "tzdata", version: "2021e", build_string: Some("he74cb21_0"), build_number: Some(0) }
        // Install - SolvableInfo { name: "xz", version: "5.2.6", build_string: Some("h166bdaf_0"), build_number: Some(0) }
        // Install - SolvableInfo { name: "zlib", version: "1.2.12", build_string: Some("h166bdaf_1"), build_number: Some(1) }
    }
}
