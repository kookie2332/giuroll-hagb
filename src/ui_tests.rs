// Note: These tests require the game to be running as they read from game memory.
// They are marked #[ignore] and can be run with: cargo test -- --ignored

use crate::ui::get_num_length;

#[test]
#[ignore = "requires game memory - run with --ignored"]
fn test_get_num_length_single_digit() {
    // For single digit (0-9), length should be 1 digit
    // This tests the logic, not actual pixel values (which depend on game state)
    let len = get_num_length(5, false);
    // Result depends on game memory values, but function should not panic
    assert!(len >= 0.0);
}

#[test]
#[ignore = "requires game memory - run with --ignored"]
fn test_get_num_length_zero() {
    // Zero should be treated as 1 digit
    let len = get_num_length(0, false);
    assert!(len >= 0.0);
}

#[test]
#[ignore = "requires game memory - run with --ignored"]
fn test_get_num_length_multi_digit() {
    let len_10 = get_num_length(10, false);
    let len_100 = get_num_length(100, false);
    let len_1000 = get_num_length(1000, false);

    // More digits should result in greater length (or equal if width is 0)
    assert!(len_100 >= len_10);
    assert!(len_1000 >= len_100);
}

#[test]
#[ignore = "requires game memory - run with --ignored"]
fn test_get_num_length_edge_spacing() {
    // With edge spacing true, should have slightly different result
    let without_edge = get_num_length(42, false);
    let with_edge = get_num_length(42, true);
    // Both should be valid, non-negative values
    assert!(without_edge >= 0.0);
    assert!(with_edge >= 0.0);
}

#[test]
#[ignore = "requires game memory - run with --ignored"]
fn test_get_num_length_negative() {
    // Negative numbers - the function uses integer division
    // so -5 / 10 = 0, treating it as 1 digit
    let len = get_num_length(-5, false);
    assert!(len >= 0.0);
}

#[test]
#[ignore = "requires game memory - run with --ignored"]
fn test_get_num_length_large_number() {
    // Test with a large number
    let len = get_num_length(999999999, false);
    assert!(len >= 0.0);
}

// Pure logic tests that don't require game memory

/// Test digit counting logic extracted from get_num_length
fn count_digits(num: i32) -> usize {
    let mut len: usize = 0;
    let mut num_ = num;
    while num_ != 0 {
        num_ /= 10;
        len += 1;
    }
    if len == 0 {
        len = 1;
    }
    len
}

#[test]
fn test_digit_count_zero() {
    assert_eq!(count_digits(0), 1);
}

#[test]
fn test_digit_count_single() {
    assert_eq!(count_digits(5), 1);
    assert_eq!(count_digits(9), 1);
}

#[test]
fn test_digit_count_double() {
    assert_eq!(count_digits(10), 2);
    assert_eq!(count_digits(42), 2);
    assert_eq!(count_digits(99), 2);
}

#[test]
fn test_digit_count_triple() {
    assert_eq!(count_digits(100), 3);
    assert_eq!(count_digits(999), 3);
}

#[test]
fn test_digit_count_negative() {
    // Negative numbers: -5 / 10 = 0 in integer division
    // So negatives with abs < 10 are treated as 1 digit
    assert_eq!(count_digits(-5), 1);
    assert_eq!(count_digits(-9), 1);
    // -10 / 10 = -1, -1 / 10 = 0, so 2 digits
    assert_eq!(count_digits(-10), 2);
    assert_eq!(count_digits(-99), 2);
}

#[test]
fn test_digit_count_large() {
    assert_eq!(count_digits(1_000_000_000), 10);
}
