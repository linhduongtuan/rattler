# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.11.0] - 2023-10-17

### ✨ Highlights

Lock file support has been moved into its own crate (rattler_lock) and support for pip packages has been added.

### 📃 Details

#### Changed

* change authentication fallback warnings to debug by @ruben-arts in https://github.com/mamba-org/rattler/pull/365
* repodata cache now uses `.info.json` instead of `.state.json` by @dholth in https://github.com/mamba-org/rattler/pull/377
* lock file now lives in its own crate with pip support by @baszalmstra in https://github.com/mamba-org/rattler/pull/378

#### Fixed
* Nushell fixes by @wolfv in https://github.com/mamba-org/rattler/pull/360
* Construct placeholder string at runtime to work around invalid conda prefix replacement by @baszalmstra in https://github.com/mamba-org/rattler/pull/371
* xonsh extension by @ruben-arts in https://github.com/mamba-org/rattler/pull/375

## New Contributors
* @dholth made their first contribution in https://github.com/mamba-org/rattler/pull/377

**Full Changelog**: https://github.com/mamba-org/rattler/compare/v0.10.0...v0.11.0

## [0.10.0] - 2023-10-02

### ✨ Highlights

The solver has been renamed and moved to its own repository: [resolvo](https://github.com/mamba-org/resolvo).
With the latest changes to the python bindings you can now download repodata and solve environments! 
Still no official release of the bindings though, but getting closer every day.

### 📃 Details

#### Added

* add initial nushell support by @wolfv in [#271](https://github.com/mamba-org/rattler/pull/271)

#### Changed

* the solver has been extracted in its own package: resolvo by @baszalmstra in [#349](https://github.com/mamba-org/rattler/pull/349) & [#350](https://github.com/mamba-org/rattler/pull/350)

#### Fixed

* Change solver implementation doc comment by @nichmor in [#352](https://github.com/mamba-org/rattler/pull/352)

### 🐍 Python

* add more py-rattler types by @Wackyator in [#348](https://github.com/mamba-org/rattler/pull/348)
* add fetch repo data to py-rattler by @Wackyator in [#334](https://github.com/mamba-org/rattler/pull/334)
* use SparseRepoData in fetch_repo_data by @Wackyator in [#359](https://github.com/mamba-org/rattler/pull/359)
* add solver by @Wackyator in [#361](https://github.com/mamba-org/rattler/pull/361)

### 🤗 New Contributors
* @nichmor made their first contribution in [#352](https://github.com/mamba-org/rattler/pull/352)

**Full Changelog**: https://github.com/mamba-org/rattler/compare/v0.9.0...v0.10.0


## [0.9.0] - 2023-09-22

### ✨ Highlights

This is a pretty substantial release which includes many refactors to the solver (which we will pull out of this repository at some point), initial work on Python bindings, and many many fixes.  

### 📃 Details

#### Added
* [pixi](https://github.com/prefix-dev/pixi) project to make contributing easier by @YeungOnion in [#283](https://github.com/mamba-org/rattler/pull/283), [#342](https://github.com/mamba-org/rattler/pull/342)
* make rattler-package-streaming compile with wasm by @wolfv in [#287](https://github.com/mamba-org/rattler/pull/287)
* implement base_url cep by @baszalmstra in [#322](https://github.com/mamba-org/rattler/pull/322)
* use emscripten-wasm32 and wasi-wasm32 by @wolfv in [#333](https://github.com/mamba-org/rattler/pull/333)
* add build_spec module by @YeungOnion in [#340](https://github.com/mamba-org/rattler/pull/340), [#346](https://github.com/mamba-org/rattler/pull/346)

#### Changed

* use normalized package names where applicable by @baszalmstra in [#285](https://github.com/mamba-org/rattler/pull/285)
* new `StrictVersion` type for VersionSpec ranges. by @tdejager in [#296](https://github.com/mamba-org/rattler/pull/296)
* refactored ratter_libsolv_rs to be conda agnostic by @tdejager & @baszalmstra in [#316](https://github.com/mamba-org/rattler/pull/316), [#309](https://github.com/mamba-org/rattler/pull/309), [#317](https://github.com/mamba-org/rattler/pull/317), [#320](https://github.com/mamba-org/rattler/pull/320), [#319](https://github.com/mamba-org/rattler/pull/319), [#323](https://github.com/mamba-org/rattler/pull/323), [#324](https://github.com/mamba-org/rattler/pull/324), [#328](https://github.com/mamba-org/rattler/pull/328), [#325](https://github.com/mamba-org/rattler/pull/325), [#326](https://github.com/mamba-org/rattler/pull/326), [#335](https://github.com/mamba-org/rattler/pull/335), [#336](https://github.com/mamba-org/rattler/pull/336), [#338](https://github.com/mamba-org/rattler/pull/338), [#343](https://github.com/mamba-org/rattler/pull/343), [#337](https://github.com/mamba-org/rattler/pull/337)
* feat: allow disabling jlap by @baszalmstra in [#327](https://github.com/mamba-org/rattler/pull/327)
* test: added job to check for lfs links by @tdejager in [#331](https://github.com/mamba-org/rattler/pull/331)
* hide implementation detail, version_spec::Constraint by @YeungOnion in [#341](https://github.com/mamba-org/rattler/pull/341)

#### Fixed

* typo in solver error message by @baszalmstra in [#284](https://github.com/mamba-org/rattler/pull/284)
* expose ParseMatchSpecError in rattler_conda_types by @Wackyator in [#286](https://github.com/mamba-org/rattler/pull/286)
* use nvidia-smi on musl targets to detect Cuda by @baszalmstra in [#290](https://github.com/mamba-org/rattler/pull/290)
* typo in snap file by @wolfv in [#291](https://github.com/mamba-org/rattler/pull/291)
* Version::is_dev returning false for dev version (fix #289) by @Wackyator in [#293](https://github.com/mamba-org/rattler/pull/293)
* workaround for `PIP_REQUIRE_VIRTUALENV` env variable by @tusharsadhwani in [#294](https://github.com/mamba-org/rattler/pull/294)
* ensure consistent sorting of locked packages by @baszalmstra in [#295](https://github.com/mamba-org/rattler/pull/295)
* updates to `NamelessMatchSpec` to allow deserializing by @travishathaway in [#299](https://github.com/mamba-org/rattler/pull/299)
* update all dependencies and fix chrono deprecation by @wolfv in [#302](https://github.com/mamba-org/rattler/pull/302)
* shell improvements for powershell env-var escaping and xonsh detection by @wolfv in [#307](https://github.com/mamba-org/rattler/pull/307)
* also export strict version by @wolfv in [#312](https://github.com/mamba-org/rattler/pull/312)
* make FetchRepoDataOptions clonable by @Wackyator in [#321](https://github.com/mamba-org/rattler/pull/321)
* bump json-patch 1.1.0 to fix stack overflow by @baszalmstra in [#332](https://github.com/mamba-org/rattler/pull/332)
* emscripten is a unix variant by @wolfv in [#339](https://github.com/mamba-org/rattler/pull/339)
* authentication fallback storage location by @ruben-arts in [#347](https://github.com/mamba-org/rattler/pull/347)

### 🐍 Python

Although this release doesn't include a formal release of the python bindings yet, a lot of work has been done to work towards a first version. 

* initial version of rattler python bindings by @baszalmstra in [#279](https://github.com/mamba-org/rattler/pull/279)
* bind `Version`, `MatchSpec`, `NamelessMatchSpec` by @Wackyator in [#292](https://github.com/mamba-org/rattler/pull/292)
* add more tests, hash and repr changes by @baszalmstra in [#300](https://github.com/mamba-org/rattler/pull/300)
* add license by @Wackyator in [#301](https://github.com/mamba-org/rattler/pull/301)
* bind channel types to py-rattler by @wolfv in [#313](https://github.com/mamba-org/rattler/pull/313)
* bind everything necessary for shell activation by @wolfv in [#298](https://github.com/mamba-org/rattler/pull/298)
* add mypy checks by @baszalmstra in [#314](https://github.com/mamba-org/rattler/pull/314)
* bind `AuthenticatedClient` by @Wackyator in [#315](https://github.com/mamba-org/rattler/pull/315)
* add `py.typed` file by @baszalmstra in [#318](https://github.com/mamba-org/rattler/pull/318)
* bind `VersionWithSource` by @Wackyator in [#304](https://github.com/mamba-org/rattler/pull/304)

### 🤗 New Contributors
* @Wackyator made their first contribution in [#286](https://github.com/mamba-org/rattler/pull/286)
* @YeungOnion made their first contribution in [#283](https://github.com/mamba-org/rattler/pull/283)
* @tusharsadhwani made their first contribution in [#294](https://github.com/mamba-org/rattler/pull/294)

To all contributors, thank you for your amazing work on Rattler. This project wouldn't exist without you! 🙏

## [0.8.0] - 2023-08-22

### Highlights

This release contains bug fixes.

### Details

#### Added

- retry behavior when downloading package archives by @baszalmstra in ([#281](https://github.com/mamba-org/rattler/pull/281))

#### Fixed

- parsing of local versions in `Constraint`s by @baszalmstra in ([#280](https://github.com/mamba-org/rattler/pull/280))

## [0.7.0] - 2023-08-11

### Highlights

This release mostly contains bug fixes.

### Details

#### Added

- Rattler is now also build for Linux aarch64 in CI by @pavelzw in ([#272](https://github.com/mamba-org/rattler/pull/272))
- `FromStr` for `ShellEnum` by @ruben-arts in ([#258](https://github.com/mamba-org/rattler/pull/258))

#### Changed

- Run activation scripts and capture their output by @baszalmstra in ([#239](https://github.com/mamba-org/rattler/pull/239))
- If memory mapping fails during installation the entire file is read instead by @baszalmstra in ([#273](https://github.com/mamba-org/rattler/pull/273))
- Constraints parsing to improve performance and error messages by @baszalmstra in ([#254](https://github.com/mamba-org/rattler/pull/254))
- Added explicit error in case repodata does not exist on the server by @ruben-arts in ([#256](https://github.com/mamba-org/rattler/pull/256))
- Code signing on apple platform now uses `codesign` binary instead of `apple-codesign` crate by @wolfv in ([#259](https://github.com/mamba-org/rattler/pull/259))

#### Fixed

- `Shell::run_command` ends with a newline by @baszalmstra in ([#262](https://github.com/mamba-org/rattler/pull/262))
- Formatting of environment variable in fish by @ruben-arts in ([#264](https://github.com/mamba-org/rattler/pull/264))
- Suppress stderr and stdout from codesigning by @wolfv in ([#265](https://github.com/mamba-org/rattler/pull/265))
- All crates have at least basic documentation by @wolfv in ([#268](https://github.com/mamba-org/rattler/pull/268))
- Use `default_cache_dir` in the rattler binary by @wolfv in ([#269](https://github.com/mamba-org/rattler/pull/269))
- Corrupted tar files generated by rattler-package-streaming by @johnhany97 in ([#276](https://github.com/mamba-org/rattler/pull/276))
- Superfluous quotes in the `content-hash` of conda lock files by @baszalmstra in ([#277](https://github.com/mamba-org/rattler/pull/277))
- `subdir` and `arch` fields when converting `RepoDataRecord` to a `LockedDependency` in conda-lock format by @wolfv in ([#255](https://github.com/mamba-org/rattler/pull/255))

## [0.6.0] - 2023-07-07

### Highlights

#### New rust based solver implementation

This version of rattler includes a new solver implementation! 
@aochagavia worked hard on porting libsolv to rust and integrating that with `rattler_solve`.
The port performs slightly faster or similar to the original C code and does not contain unsafe code, is well documented, and thread-safe.
Our implementation (`rattler_libsolv_rs`) is specific to solving conda packages by leveraging `rattler_conda_types` for matching and parsing.

Some performance benchmarks taken on Apple M2 Max.

|                                    | libsolv-c    | libsolv-rs    |
| ---------------------------------- |--------------|---------------|
| python=3.9                         | 7.3734 ms    | **4.5831 ms** |
| xtensor, xsimd                     | 5.7521 ms    | **2.7643 ms** |
| tensorflow                         | 654.38 ms    | **371.59 ms** |
| quetz                              | **1.2577 s** | 1.3807 s      |
| tensorboard=2.1.1, grpc-cpp=1.39.1 | 474.76 ms    | **132.79 ms** |

> Run `cargo bench libsolv` to check the results on your own machine.

Besides the much improved implementation the new solver also provides much better error messages based on the work from mamba.
When a conflict is detected the incompatibilities are analyzed and displayed with a more user-friendly error message.

```
The following packages are incompatible
|-- asdf can be installed with any of the following options:
    |-- asdf 1.2.3 would require
        |-- C >1, which can be installed with any of the following options:
            |-- C 2.0.0
|-- C 1.0.0 is locked, but another version is required as reported above
```

`rattler-solve` has also been refactored to accommodate this change.
It is now more easily possible to switch between solvers add runtime by writing functions that are generic on the solver.
The solvers now live in a separate module `rattler_solve::libsolv_c` for the original libsolv C implementation and `rattler_solve::libsolv_rs` for the rust version.
Both solvers can be enabled with feature flags. The default features only select `libsolv_c`.

### Caching of activation scripts

This release contains code to execute an activation script and capture the changes it made to the environment.
Caching the result of an activation script can be useful if you need to invoke multiple executables from the same environment.

### Details

#### Added

- Run activation scripts and capture their output by @baszalmstra in ([#239](https://github.com/mamba-org/rattler/pull/239))
- Support for sha256 and md5 field in matchspec by @0xbe7a in ([#241](https://github.com/mamba-org/rattler/pull/241))
- A rust port of libsolv as an additional solver backend by @aochagavia, @baszalmstra in ([#243](https://github.com/mamba-org/rattler/pull/243) & [#253](https://github.com/mamba-org/rattler/pull/253)) 
- Test cases and benchmarks for solver implementations by @baszalmstra in ([#250](https://github.com/mamba-org/rattler/pull/249) & [#250](https://github.com/mamba-org/rattler/pull/249))
- The ability to add a dependency from `python` on `pip` while loading repodata @wolfv in ([#238](https://github.com/mamba-org/rattler/pull/238))

#### Changed

- Completely refactored version parsing by @baszalmstra in ([#240](https://github.com/mamba-org/rattler/pull/240))
- Refactored solver interface to allow generic use of solver implementations by @baszalmstra in ([#245](https://github.com/mamba-org/rattler/pull/245))
- Also check if credentials stored under wildcard host by @wolfv in ([#252](https://github.com/mamba-org/rattler/pull/252))

#### Fixed

- Compilation issues by @wolfv in ([#244](https://github.com/mamba-org/rattler/pull/244))
- Add missing `From<VersionWithSource>` for `Version` by @baszalmstra in ([#246](https://github.com/mamba-org/rattler/pull/246))
- Optimized libsolv port by removing redundant MatchSpec parsing by @baszalmstra in ([#246](https://github.com/mamba-org/rattler/pull/246))
- Optimized libsolv port by caching matching Solvables by @baszalmstra in ([#251](https://github.com/mamba-org/rattler/pull/251))

## [0.5.0] - 2023-06-30

### Highlights

A bug fix release

### Details

#### Added

- More control over how the PATH is altered during activation ([#232](https://github.com/mamba-org/rattler/pull/232))

#### Fixed

- Reconstructing of RepoData from conda lock files for local channels ([#231](https://github.com/mamba-org/rattler/pull/231))
- Powershell on Linux ([#234](https://github.com/mamba-org/rattler/pull/234))
- Proper parsing of `>2.10*` as `>=2.10` ([#237](https://github.com/mamba-org/rattler/pull/237))

## [0.4.0] - 2023-06-23

### Highlights

A new algorithm was introduced to sort `PackageRecord`s in a topological order based on their dependencies.
Sorting in this way provides a deterministic way of sorting packages in the order in which they should be installed to avoid clobbering.
The original algorithm was extracted from [rattler-server](https://github.com/mamba-org/rattler-server).

Experimental extensions to the conda lock file format have also been introduced to make it possible to completely reproduce the original `RepoDataRecord`s from a lock file.

Fixes were made to the `MatchSpec` and `Version` implementation to catch some corner cases and detecting the current shell has become more robust.

### Details

#### Added

- `PackageRecord::sort_topologically` to perform a topological sort of `PackageRecord`s ([#218](https://github.com/mamba-org/rattler/pull/218))
- Experimental fields to be able to reconstruct `RepoDataRecord` from conda lock files. ([#221](https://github.com/mamba-org/rattler/pull/221))
- Methods to manipulate `Version`s ([#229](https://github.com/mamba-org/rattler/pull/229))

#### Changed

- Refactored shell detection code using `$SHELL` or parent process name ([#219](https://github.com/mamba-org/rattler/pull/219))
- The error message that is thrown when parsing a `Platform` now includes possible options ([#222](https://github.com/mamba-org/rattler/pull/222))
- Completely refactored `Version` implementation to reduce memory footprint and increase readability ([#227](https://github.com/mamba-org/rattler/pull/227))

#### Fixed

- Issue with parsing matchspecs that contain epochs ([#220](https://github.com/mamba-org/rattler/pull/220))
- Zsh activation scripts invoke .sh scripts ([#223](https://github.com/mamba-org/rattler/pull/223))
- Detect the proper powershell parent process ([#224](https://github.com/mamba-org/rattler/pull/224))

## [0.3.0] - 2023-06-15

### Highlights

This release contains lots of fixes, small (breaking) changes, and new features.
The biggest highlights are:

#### JLAP support

JLAP is a file format to incrementally update a cached `repodata.json` without downloading the entire file.
This can save a huge amount of bandwidth for large repodatas that change often (like those from conda-forge).
If you have a previously cached `repodata.json` on your system only small JSON patches are downloaded to bring your cache up to date.
The format was initially proposed through a [CEP](https://github.com/conda-incubator/ceps/pull/20) and has been available in conda as an experimental feature since `23.3.0`.

When using rattler you get JLAP support out of the box.
No changes are needed.

#### Support for local `file://`

`file://` based urls are now supported for all functions that use a Url to download certain data.

#### `rattler_networking`

A new crate has been added to facilitate authentication when downloading repodata or packages called `rattler_networking`.

### Details

#### Added

- Support for detecting more platforms ([#135](https://github.com/mamba-org/rattler/pull/135))
- `RepoData` is now clonable ([#138](https://github.com/mamba-org/rattler/pull/138))
- `RunExportsJson` is now clonable ([#169](https://github.com/mamba-org/rattler/pull/169))
- `file://` urls are now supported for package extraction functions ([#157](https://github.com/mamba-org/rattler/pull/157))
- `file://` urls are now supported for repodata fetching ([#158](https://github.com/mamba-org/rattler/pull/158))
- Getting started with rattler using micromamba ([#163](https://github.com/mamba-org/rattler/pull/163))
- Add `Platform::arch` function to return the architecture of a given platform ([#166](https://github.com/mamba-org/rattler/pull/166))
- Extracted Python style JSON formatting into [a separate crate](https://github.com/prefix-dev/serde-json-python-formatter) ([#163](https://github.com/mamba-org/rattler/pull/180))
- Added feature to use `rustls` with `rattler_package_streaming` and `rattler_repodata_gateway` ([#179](https://github.com/mamba-org/rattler/pull/179) & [#181](https://github.com/mamba-org/rattler/pull/181))
- Expose `version_spec` module ([#183](https://github.com/mamba-org/rattler/pull/183))
- `NamelessMatchSpec` a variant of `MatchSpec` that does not include a package name [#185](https://github.com/mamba-org/rattler/pull/185))
- `ShellEnum` - a dynamic shell type for dynamic discovery [#187](https://github.com/mamba-org/rattler/pull/187))
- Exposed the `python_entry_point_template` function ([#190](https://github.com/mamba-org/rattler/pull/190))
- Enable deserializing virtual packages ([#198](https://github.com/mamba-org/rattler/pull/198))
- Refactored CI to add macOS arm64 ([#201](https://github.com/mamba-org/rattler/pull/201))
- Support for JLAP when downloading repodata ([#197](https://github.com/mamba-org/rattler/pull/197) & [#214](https://github.com/mamba-org/rattler/pull/214))
- `Clone`, `Debug`, `PartialEq`, `Eq` implementations for conda lock types ([#213](https://github.com/mamba-org/rattler/pull/213))
- `rattler_networking` to enable accessing `repodata.json` and packages that require authentication ([#191](https://github.com/mamba-org/rattler/pull/191))

#### Changed

- `FileMode` is now included with `prefix_placeholder` is set ([#136](https://github.com/mamba-org/rattler/pull/135))
- `rattler_digest` now re-exports commonly used hash types and typed hashes are now used in more placed (instead of strings) [[#137](https://github.com/mamba-org/rattler/pull/137) & [#153](https://github.com/mamba-org/rattler/pull/153)]
- Use `Platform` in to detect running operating system ([#144](https://github.com/mamba-org/rattler/pull/144))
- `paths.json` is now serialized in a deterministic fashion ([#147](https://github.com/mamba-org/rattler/pull/147))
- Determine the `subdir` for the `platform` and `arch` fields when creating a `PackageRecord` from an `index.json` ([#145](https://github.com/mamba-org/rattler/pull/145) & [#152](https://github.com/mamba-org/rattler/pull/152))
- `Activator::activation` now returns the new `PATH` in addition to the script ([#151](https://github.com/mamba-org/rattler/pull/151))
- Use properly typed `chrono::DateTime<chrono::Utc>` for timestamps instead of `u64` ([#157](https://github.com/mamba-org/rattler/pull/157))
- Made `ParseError` public and reuse `ArchiveType` ([#167](https://github.com/mamba-org/rattler/pull/167))
- Allow setting timestamps when creating package archive ([#171](https://github.com/mamba-org/rattler/pull/171))
- `about.json` and `index.json` are now serialized in a deterministic fashion ([#180](https://github.com/mamba-org/rattler/pull/180))
- SHA256 and MD5 hashes are computed on the fly when extracting packages ([#176](https://github.com/mamba-org/rattler/pull/176)
- Change blake2 hash to use blake2b instead of blake2s ([#192](https://github.com/mamba-org/rattler/pull/192)
- LibSolv error messages are now passed through ([#202](https://github.com/mamba-org/rattler/pull/202) & [#210](https://github.com/mamba-org/rattler/pull/210))
- `VersionTree` parsing now uses `nom` instead of a complex regex ([#206](https://github.com/mamba-org/rattler/pull/206)
- `libc` version detection now uses `lld --version` to properly detect the libc version on the host ([#209](https://github.com/mamba-org/rattler/pull/209)
- Improved version parse error messages ([#211](https://github.com/mamba-org/rattler/pull/211)
- Parsing of some complex MatchSpecs ([#217](https://github.com/mamba-org/rattler/pull/217)

#### Fixed

- MatchSpec bracket list parsing can now handle quoted values ([#157](https://github.com/mamba-org/rattler/pull/156))
- Typos and documentation ([#164](https://github.com/mamba-org/rattler/pull/164) & [#188](https://github.com/mamba-org/rattler/pull/188))
- Allow downloading of repodata.json to fail in some cases (only noarch is a required subdir) ([#174](https://github.com/mamba-org/rattler/pull/174))
- Missing feature when using the sparse index ([#182](https://github.com/mamba-org/rattler/pull/182))
- Several small issues or missing functionality ([#184](https://github.com/mamba-org/rattler/pull/184))
- Loosened strictness of comparing packages in `Transaction`s ([#186](https://github.com/mamba-org/rattler/pull/186)
- Missing `noarch: generic` parsing in `links.json` ([#189](https://github.com/mamba-org/rattler/pull/189)
- Ignore trailing .0 in version comparison ([#196](https://github.com/mamba-org/rattler/pull/196)

## [0.2.0] - 2023-03-24

### Added

- Construction methods for NoArchType (#130)
- Function to create package record from index.json + size and hashes (#126)
- Serialization for repodata (#124)
- Functions to apply patches to repodata (#127)
- More tests and evict removed packages from repodata (#128)
- First version of package writing functions (#112)

### Changed

- Removed dependency on `clang-sys` during build (#131)

## [0.1.0] - 2023-03-16

First release

[unreleased]: https://github.com/mamba-org/rattler/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/mamba-org/rattler/releases/tag/v0.1.0
