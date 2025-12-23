use crate::timing::{
    target_frametime, F62_ENABLED, SOKU_LOOP_EVENT, VERSION_BYTE_60, VERSION_BYTE_62,
    WARNING_FRAME_LOST_COUNTDOWN,
};

#[test]
fn test_target_frametime_60fps() {
    unsafe {
        F62_ENABLED = false;
        let frametime = target_frametime();
        // 1,000,000 / 60 = 16666 microseconds per frame
        assert_eq!(frametime, 16666);
    }
}

#[test]
fn test_target_frametime_62fps() {
    unsafe {
        F62_ENABLED = true;
        let frametime = target_frametime();
        // 1,000,000 / 62 = 16129 microseconds per frame
        assert_eq!(frametime, 16129);
        // Reset to default
        F62_ENABLED = false;
    }
}

#[test]
fn test_version_byte_constants() {
    assert_eq!(VERSION_BYTE_60, 0x6b);
    assert_eq!(VERSION_BYTE_62, 0x6c);
    // Ensure they are different
    assert_ne!(VERSION_BYTE_60, VERSION_BYTE_62);
}

#[test]
fn test_frametime_difference() {
    unsafe {
        F62_ENABLED = false;
        let time_60 = target_frametime();
        F62_ENABLED = true;
        let time_62 = target_frametime();
        F62_ENABLED = false;

        // 62fps should have a shorter frame time than 60fps
        assert!(time_62 < time_60);
        // Difference should be around 537 microseconds (16666 - 16129)
        assert_eq!(time_60 - time_62, 537);
    }
}

#[test]
fn test_soku_loop_event_mutex() {
    // Test that we can acquire the mutex and it starts as None
    let guard = SOKU_LOOP_EVENT.lock().unwrap();
    // Just verify we can lock it - the actual value depends on runtime state
    drop(guard);

    // Test setting a value
    {
        let mut guard = SOKU_LOOP_EVENT.lock().unwrap();
        *guard = Some(12345);
    }
    {
        let guard = SOKU_LOOP_EVENT.lock().unwrap();
        assert_eq!(*guard, Some(12345));
    }
    // Clean up
    {
        let mut guard = SOKU_LOOP_EVENT.lock().unwrap();
        *guard = None;
    }
}

#[test]
fn test_warning_frame_lost_countdown_atomic() {
    use std::sync::atomic::Ordering::Relaxed;

    unsafe {
        // Test store and load
        WARNING_FRAME_LOST_COUNTDOWN.store(100, Relaxed);
        assert_eq!(WARNING_FRAME_LOST_COUNTDOWN.load(Relaxed), 100);

        // Test fetch_sub
        WARNING_FRAME_LOST_COUNTDOWN.fetch_sub(1, Relaxed);
        assert_eq!(WARNING_FRAME_LOST_COUNTDOWN.load(Relaxed), 99);

        // Reset
        WARNING_FRAME_LOST_COUNTDOWN.store(0, Relaxed);
    }
}
