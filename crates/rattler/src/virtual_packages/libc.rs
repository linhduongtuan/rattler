use crate::Version;
use once_cell::sync::Lazy;
use std::{
    ffi::{CString, FromVecWithNulError, IntoStringError},
    os::raw::c_int,
    str::FromStr,
};
use tracing::log;

mod ffi {
    use std::os::raw::{c_char, c_int};

    pub const CS_GNU_LIBC_VERSION: c_int = 2;
    pub const CS_GNU_LIBPTHREAD_VERSION: c_int = 3;

    extern "C" {
        /// Get configuration dependent string variables
        pub fn confstr(name: c_int, buf: *mut c_char, length: libc::size_t) -> libc::size_t;
    }
}

/// Memoized libc version
pub static DETECTED_LIBC_VERSION: Lazy<Option<(String, Version)>> = Lazy::new(detect_libc_version);

/// Tries to detect the libc version used by the system.
pub fn detect_libc_version() -> Option<(String, Version)> {
    // Use confstr to determine the LibC family and version
    let version = [ffi::CS_GNU_LIBC_VERSION, ffi::CS_GNU_LIBPTHREAD_VERSION]
        .into_iter()
        .find_map(|name| confstr(name).unwrap_or(None))?;

    // Split into family and version
    let (family, version) = version.split_once(' ')?;

    // Parse the version string
    let version = match Version::from_str(version) {
        Ok(version) => version,
        Err(e) => {
            log::warn!("unable to parse libc version: {e}");
            return None;
        }
    };

    // The family might be NPTL but thats just the name of the threading library, even though the
    // version refers to that of uClibc.
    if family == "NPTL" {
        let family = String::from("uClibc");
        log::warn!(
            "failed to detect non-glibc family, assuming {} ({})",
            &family,
            &version
        );
        Some((family, version))
    } else {
        Some((family.to_owned(), version))
    }
}

/// A possible error returned by `confstr`.
#[derive(Debug, thiserror::Error)]
enum ConfStrError {
    #[error("invalid string returned: {0}")]
    FromVecWithNulError(#[from] FromVecWithNulError),

    #[error("invalid utf8 string: {0}")]
    InvalidUtf8String(#[from] IntoStringError),
}

/// Safe wrapper around `confstr`
fn confstr(name: c_int) -> Result<Option<String>, ConfStrError> {
    let len = match unsafe { ffi::confstr(name, std::ptr::null_mut(), 0) } {
        0 => return Ok(None),
        len => len,
    };
    let mut bytes = vec![0u8; len];
    if unsafe { ffi::confstr(name, bytes.as_mut_ptr() as *mut _, bytes.len()) } == 0 {
        return Ok(None);
    }
    Ok(Some(CString::from_vec_with_nul(bytes)?.into_string()?))
}

#[cfg(test)]
mod test {
    use super::detect_libc_version;

    #[test]
    pub fn doesnt_crash() {
        let version = detect_libc_version();
        println!("{:?}", version);
    }
}
