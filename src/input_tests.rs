use crate::input::input_to_accum;
use crate::INPUT_KEYS_NUMBERS;

#[test]
fn test_input_to_accum_empty() {
    let input = [false; INPUT_KEYS_NUMBERS];
    assert_eq!(input_to_accum(&input), 0);
}

#[test]
fn test_input_to_accum_first_bit() {
    let mut input = [false; INPUT_KEYS_NUMBERS];
    input[0] = true;
    assert_eq!(input_to_accum(&input), 1);
}

#[test]
fn test_input_to_accum_multiple_bits() {
    let mut input = [false; INPUT_KEYS_NUMBERS];
    input[0] = true;
    input[2] = true;
    input[4] = true;
    assert_eq!(input_to_accum(&input), 0b10101);
}

#[test]
fn test_input_to_accum_all_bits() {
    let input = [true; INPUT_KEYS_NUMBERS];
    // All 12 bits set = 0b111111111111 = 0xFFF = 4095
    assert_eq!(input_to_accum(&input), 0xFFF);
}

#[test]
fn test_input_to_accum_last_bit_only() {
    let mut input = [false; INPUT_KEYS_NUMBERS];
    input[INPUT_KEYS_NUMBERS - 1] = true;
    assert_eq!(input_to_accum(&input), 1 << (INPUT_KEYS_NUMBERS - 1));
}

#[test]
fn test_input_to_accum_direction_keys() {
    // Test typical direction input pattern (left + up)
    let mut input = [false; INPUT_KEYS_NUMBERS];
    input[0] = true; // left
    input[2] = true; // up
    assert_eq!(input_to_accum(&input), 0b0101);
}

#[test]
fn test_input_to_accum_button_keys() {
    // Test button inputs (indices 4-11)
    let mut input = [false; INPUT_KEYS_NUMBERS];
    input[4] = true; // A button
    input[5] = true; // B button
    input[6] = true; // C button
    assert_eq!(input_to_accum(&input), 0b1110000);
}

#[test]
fn test_input_to_accum_idempotent() {
    let mut input = [false; INPUT_KEYS_NUMBERS];
    input[3] = true;
    input[7] = true;
    let result1 = input_to_accum(&input);
    let result2 = input_to_accum(&input);
    assert_eq!(result1, result2);
}

#[test]
fn test_input_keys_numbers_constant() {
    // Verify the constant is what we expect
    assert_eq!(INPUT_KEYS_NUMBERS, 12);
}
