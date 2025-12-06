use std::sync::atomic::Ordering::Relaxed;

use crate::game_state::{
    pause, resume, DISABLE_SEND, ESC2, LAST_STATE, TARGET_OFFSET,
    CENTER_X_P1, CENTER_X_P2, CENTER_Y_P1, CENTER_Y_P2,
    INSIDE_HALF_HEIGHT, INSIDE_HALF_WIDTH, MAX_ROLLBACK_PREFERENCE,
    OUTER_HALF_HEIGHT, OUTER_HALF_WIDTH,
};

#[test]
fn test_pause_changes_state_to_4() {
    LAST_STATE.store(0x6b, Relaxed);
    let mut battle_state: u32 = 2;
    let mut state_sub_count: u32 = 10;

    pause(&mut battle_state, &mut state_sub_count);

    assert_eq!(battle_state, 4);
    assert_eq!(state_sub_count, 9);
    assert_eq!(LAST_STATE.load(Relaxed), 2);
}

#[test]
fn test_pause_does_nothing_when_already_paused() {
    LAST_STATE.store(0x6b, Relaxed);
    let mut battle_state: u32 = 4;
    let mut state_sub_count: u32 = 10;

    pause(&mut battle_state, &mut state_sub_count);

    // Should not change anything
    assert_eq!(battle_state, 4);
    assert_eq!(state_sub_count, 10);
    assert_eq!(LAST_STATE.load(Relaxed), 0x6b);
}

#[test]
fn test_resume_restores_previous_state() {
    LAST_STATE.store(3, Relaxed);
    let mut battle_state: u32 = 4;

    resume(&mut battle_state);

    assert_eq!(battle_state, 3);
    assert_eq!(LAST_STATE.load(Relaxed), 0x6b);
}

#[test]
fn test_resume_does_nothing_when_not_paused() {
    LAST_STATE.store(3, Relaxed);
    let mut battle_state: u32 = 2; // Not paused (4)

    resume(&mut battle_state);

    // Should not change battle_state
    assert_eq!(battle_state, 2);
    assert_eq!(LAST_STATE.load(Relaxed), 3);
}

#[test]
fn test_resume_does_nothing_with_default_last_state() {
    LAST_STATE.store(0x6b, Relaxed);
    let mut battle_state: u32 = 4;

    resume(&mut battle_state);

    // Should not change because LAST_STATE is 0x6b
    assert_eq!(battle_state, 4);
}

#[test]
fn test_pause_resume_roundtrip() {
    LAST_STATE.store(0x6b, Relaxed);
    let mut battle_state: u32 = 5;
    let mut state_sub_count: u32 = 100;

    pause(&mut battle_state, &mut state_sub_count);
    assert_eq!(battle_state, 4);

    resume(&mut battle_state);
    assert_eq!(battle_state, 5);
}

#[test]
fn test_pause_sub_count_wraps() {
    LAST_STATE.store(0x6b, Relaxed);
    let mut battle_state: u32 = 1;
    let mut state_sub_count: u32 = 0;

    pause(&mut battle_state, &mut state_sub_count);

    // wrapping_sub(1) on 0 should give u32::MAX
    assert_eq!(state_sub_count, u32::MAX);
}

#[test]
fn test_atomic_operations() {
    // Test ESC2 atomic
    ESC2.store(5, Relaxed);
    assert_eq!(ESC2.load(Relaxed), 5);

    // Test DISABLE_SEND atomic
    DISABLE_SEND.store(100, Relaxed);
    assert_eq!(DISABLE_SEND.load(Relaxed), 100);

    // Test TARGET_OFFSET atomic
    TARGET_OFFSET.store(-500, Relaxed);
    assert_eq!(TARGET_OFFSET.load(Relaxed), -500);

    // Cleanup
    ESC2.store(0, Relaxed);
    DISABLE_SEND.store(0, Relaxed);
    TARGET_OFFSET.store(0, Relaxed);
}

#[test]
fn test_default_values() {
    unsafe {
        // Verify some important defaults
        assert_eq!(MAX_ROLLBACK_PREFERENCE, 6);
        assert_eq!(CENTER_X_P1, 224);
        assert_eq!(CENTER_Y_P1, 428);
        assert_eq!(CENTER_X_P2, 640 - 224);
        assert_eq!(CENTER_Y_P2, 428);
        assert_eq!(INSIDE_HALF_HEIGHT, 7);
        assert_eq!(INSIDE_HALF_WIDTH, 58);
        assert_eq!(OUTER_HALF_HEIGHT, 9);
        assert_eq!(OUTER_HALF_WIDTH, 60);
    }
}
