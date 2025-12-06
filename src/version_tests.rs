use std::ffi::{CStr, CString};

use crate::version::{
    compareVersion, compareVersionString, getPriority, getVersion, getVersionString, VERSION_STR,
};

#[test]
fn test_version_str_not_empty() {
    assert!(!VERSION_STR.is_empty());
}

#[test]
fn test_version_str_is_semver_format() {
    // VERSION_STR should be in format X.Y.Z or X.Y.Z-suffix
    let parts: Vec<&str> = VERSION_STR.split('-').next().unwrap().split('.').collect();
    assert!(parts.len() >= 2, "Version should have at least major.minor");
    assert!(parts.len() <= 3, "Version should have at most major.minor.patch");

    for part in parts {
        assert!(part.parse::<u32>().is_ok(), "Version components should be numbers");
    }
}

#[test]
fn test_get_priority_returns_expected() {
    assert_eq!(getPriority(), 1000);
}

#[test]
fn test_get_version_returns_valid_value() {
    unsafe {
        let version = getVersion();
        // Version should be non-zero (has at least major version)
        assert!(version > 0);
    }
}

#[test]
fn test_compare_version_equal() {
    unsafe {
        let current_version = getVersion();
        let result = compareVersion(current_version);
        assert_eq!(result, 0, "Comparing version to itself should return 0");
    }
}

#[test]
fn test_compare_version_greater() {
    unsafe {
        // Compare to version 0, current should be greater
        let result = compareVersion(0);
        assert_eq!(result, 1, "Current version should be greater than 0");
    }
}

#[test]
fn test_compare_version_less() {
    unsafe {
        // Compare to max version, current should be less
        let result = compareVersion(u64::MAX);
        assert_eq!(result, -1, "Current version should be less than max");
    }
}

#[test]
fn test_get_version_string_not_null() {
    let ptr = getVersionString();
    assert!(!ptr.is_null());
}

#[test]
fn test_get_version_string_matches_version_str() {
    let ptr = getVersionString();
    let c_str = unsafe { CStr::from_ptr(ptr) };
    let str_slice = c_str.to_str().expect("Should be valid UTF-8");
    assert_eq!(str_slice, VERSION_STR);
}

#[test]
fn test_get_version_string_returns_same_pointer() {
    // getVersionString uses a static, should return same pointer each time
    let ptr1 = getVersionString();
    let ptr2 = getVersionString();
    assert_eq!(ptr1, ptr2, "Should return same cached pointer");
}

#[test]
fn test_compare_version_string_with_same_version() {
    unsafe {
        let mut result: i32 = -999;
        let version_cstring = CString::new(VERSION_STR).unwrap();
        let success = compareVersionString(version_cstring.as_ptr(), &mut result);

        assert!(success, "Should return true for valid version string");
        assert_eq!(result, 0, "Same version should compare equal");
    }
}

#[test]
fn test_compare_version_string_with_lower_version() {
    unsafe {
        let mut result: i32 = -999;
        let version_cstring = CString::new("0.0.1").unwrap();
        let success = compareVersionString(version_cstring.as_ptr(), &mut result);

        assert!(success, "Should return true for valid version string");
        assert_eq!(result, 1, "Current version should be greater than 0.0.1");
    }
}

#[test]
fn test_compare_version_string_with_higher_version() {
    unsafe {
        let mut result: i32 = -999;
        let version_cstring = CString::new("999.999.999").unwrap();
        let success = compareVersionString(version_cstring.as_ptr(), &mut result);

        assert!(success, "Should return true for valid version string");
        assert_eq!(result, -1, "Current version should be less than 999.999.999");
    }
}

#[test]
fn test_compare_version_string_with_invalid_version() {
    unsafe {
        let mut result: i32 = -999;
        let version_cstring = CString::new("not_a_version").unwrap();
        let success = compareVersionString(version_cstring.as_ptr(), &mut result);

        assert!(!success, "Should return false for invalid version string");
        // result should be unchanged
        assert_eq!(result, -999);
    }
}
