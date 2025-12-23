use std::{
    ffi::{c_char, CStr, CString},
    sync::Mutex,
};

use version_compare::{Cmp, Version};

pub const VERSION_STR: &str = env!("CARGO_PKG_VERSION");

/// Compare GR version with version_string, following Semantic Versioning 2.0.0 (https://semver.org/).
/// It returns false if version_string is an invalid version string, or
/// returns true and assign *result = 0 (GR version = version_str), -1 (GR version < version_str), or 1 (GR version > version_str), if version_str is valid
#[no_mangle]
pub unsafe extern "C" fn compareVersionString(
    version_string: *const c_char,
    result: *mut i32,
) -> bool {
    if let Ok(version_str) = CStr::from_ptr(version_string).to_str() {
        if let Some(ver) = Version::from(version_str) {
            *result = match Version::from(VERSION_STR).unwrap().compare(ver) {
                Cmp::Eq => 0,
                Cmp::Lt => -1,
                Cmp::Gt => 1,
                _ => unreachable!(),
            };
            return true;
        }
    }
    return false;
}

/// Returns version string
#[no_mangle]
pub extern "C" fn getVersionString() -> *const c_char {
    static VERSION_CSTRING: Mutex<Option<CString>> = Mutex::new(None);
    let mut string = VERSION_CSTRING.lock().unwrap();
    if string.is_none() {
        *string = Some(CString::new(VERSION_STR).unwrap());
    }
    string.as_ref().unwrap().as_ptr()
}

/// Return GR version with given version, where version values consist of four 16 bit words, e.g.
/// `MAJOR << 48 | MINOR << 32 | PATCH << 16 | RELEASE`.
#[no_mangle]
pub unsafe extern "C" fn getVersion() -> u64 {
    return env!("DLL_VERSION").parse::<u64>().unwrap();
}

/// Compare GR version with given version, where version values consist of four 16 bit words, e.g.
/// `MAJOR << 48 | MINOR << 32 | PATCH << 16 | RELEASE`.
/// It returns 0 (GR version = version_str), -1 (GR version < version_str), or 1 (GR version > version_str).
#[no_mangle]
pub unsafe extern "C" fn compareVersion(version: u64) -> i32 {
    match env!("DLL_VERSION").parse::<u64>().unwrap().cmp(&version) {
        std::cmp::Ordering::Equal => 0,
        std::cmp::Ordering::Less => -1,
        std::cmp::Ordering::Greater => 1,
    }
}

#[no_mangle]
pub extern "C" fn getPriority() -> i32 {
    1000
}
