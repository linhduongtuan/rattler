use std::os::raw::{c_char, c_int};

#[cfg(unix)]
extern "C" {
    /// Get configuration dependent string variables
    pub fn confstr(
        name: c_int,
        buf: *mut c_char,
        length: libc::size_t,
    ) -> libc::size_t;
}

const CS_GNU_LIBC_VERSION: c_int = 2;
const CS_GNU_LIBPTHREAD_VERSION: c_int = 3;

/// Tries to detect the libc version used by the system.
#[cfg(unix)]
fn detect_libc_version() -> Option<(String, Version)> {

    

    None
}
