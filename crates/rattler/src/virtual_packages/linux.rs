use crate::{ParseVersionError, Version};
use once_cell::sync::Lazy;
use std::{ffi::CStr, mem::MaybeUninit, str::FromStr};
use tracing::log;

mod ffi {
    use std::os::raw::{c_char, c_int};

    extern "C" {
        pub fn uname(buf: *mut utsname) -> c_int;
    }

    #[repr(C)]
    pub struct utsname {
        pub sysname: [c_char; 65],
        pub nodename: [c_char; 65],
        pub release: [c_char; 65],
        pub version: [c_char; 65],
        pub machine: [c_char; 65],
        pub domainname: [c_char; 65],
    }
}

/// Memoized linux version
pub static DETECTED_LINUX_VERSION: Lazy<Option<Version>> = Lazy::new(detect_linux_version);

/// Detects the current linux version.
pub fn detect_linux_version() -> Option<Version> {
    // Run the uname function to determine platform information
    let mut info = MaybeUninit::uninit();
    if unsafe { ffi::uname(info.as_mut_ptr()) } != 0 {
        return None;
    }
    let info: ffi::utsname = unsafe { info.assume_init() };

    // Get the version string
    let release_str = unsafe { CStr::from_ptr(info.release.as_ptr()) }.to_string_lossy();

    // Parse the version string
    match parse_linux_version(release_str.as_ref()) {
        Ok(version) => Some(version),
        Err(e) => {
            log::warn!(
                "unable to parse linux release version '{}': {e}",
                release_str.as_ref()
            );
            None
        }
    }
}

#[derive(Debug, Clone, thiserror::Error, PartialEq, Eq)]
enum ParseLinuxVersionError {
    #[error("error parsing linux version")]
    ParseError,

    #[error("invalid version")]
    InvalidVersion(#[from] ParseVersionError),
}

/// Returns the parsed version of the linux uname string.
fn parse_linux_version(version_str: &str) -> Result<Version, ParseLinuxVersionError> {
    Ok(Version::from_str(
        extract_linux_version_part(version_str).ok_or(ParseLinuxVersionError::ParseError)?,
    )?)
}

/// Takes the first 2, 3, or 4 digits of the linux uname version.
fn extract_linux_version_part(version_str: &str) -> Option<&str> {
    use nom::character::complete::*;
    use nom::combinator::*;
    use nom::sequence::*;
    let result: Result<_, nom::Err<nom::error::Error<_>>> = recognize(tuple((
        digit1,
        char('.'),
        digit1,
        opt(pair(char('.'), digit1)),
        opt(pair(char('.'), digit1)),
    )))(version_str);
    let (_rest, version_part) = result.ok()?;

    Some(version_part)
}

#[cfg(test)]
mod test {
    use super::{detect_linux_version, extract_linux_version_part};

    #[test]
    pub fn test_extract_linux_version_part() {
        assert_eq!(
            extract_linux_version_part("5.10.102.1-microsoft-standard-WSL2"),
            Some("5.10.102.1")
        );
        assert_eq!(
            extract_linux_version_part("2.6.32-220.17.1.el6.i686"),
            Some("2.6.32")
        );
        assert_eq!(
            extract_linux_version_part("5.4.72-microsoft-standard-WSL2"),
            Some("5.4.72")
        );
        assert_eq!(
            extract_linux_version_part("4.9.43-1-MANJARO"),
            Some("4.9.43")
        );
        assert_eq!(
            extract_linux_version_part("3.16.0-31-generic"),
            Some("3.16.0")
        );
    }

    #[test]
    pub fn doesnt_crash() {
        let version = detect_linux_version();
        println!("{:?}", version);
    }
}
