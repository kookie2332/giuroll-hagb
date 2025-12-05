use super::{input_to_accum, INPUT_KEYS_NUMBERS};

#[test]
fn converts_boolean_array_to_bitmask() {
    let mut input = [false; INPUT_KEYS_NUMBERS];
    input[0] = true;
    input[3] = true;
    input[5] = true;
    input[INPUT_KEYS_NUMBERS - 1] = true;

    let expected_mask =
        (1u16 << 0) | (1u16 << 3) | (1u16 << 5) | (1u16 << (INPUT_KEYS_NUMBERS - 1));

    assert_eq!(input_to_accum(&input), expected_mask);
}

#[test]
fn returns_zero_for_empty_input() {
    let input = [false; INPUT_KEYS_NUMBERS];
    assert_eq!(input_to_accum(&input), 0);
}
