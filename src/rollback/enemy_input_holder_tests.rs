use super::{EnemyInputHolder, INPUT_KEYS_NUMBERS};

fn make_input(indices: &[usize]) -> [bool; INPUT_KEYS_NUMBERS] {
    let mut input = [false; INPUT_KEYS_NUMBERS];
    for &idx in indices {
        input[idx] = true;
    }
    input
}

#[test]
fn missing_inputs_repeat_previous_value() {
    let mut holder = EnemyInputHolder::new();

    // Frame 0 defaults to all false and subsequent empty frames repeat the previous value.
    let baseline = holder.get(0);
    assert!(baseline.iter().all(|v| !v));
    assert_eq!(holder.get(1), baseline);

    let expected = make_input(&[1, 4, INPUT_KEYS_NUMBERS - 1]);
    holder.insert(expected, 2);

    // Inserted frame is returned and later missing frames inherit it.
    assert_eq!(holder.get(2), expected);
    assert_eq!(holder.get(3), expected);
}

#[test]
fn reinserting_same_input_is_allowed() {
    let mut holder = EnemyInputHolder::new();
    let expected = make_input(&[0, 2]);

    holder.insert(expected, 1);
    // Reinserting the exact same value should not panic and should preserve the data.
    holder.insert(expected, 1);

    assert_eq!(holder.get(1), expected);
}
