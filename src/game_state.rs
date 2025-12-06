//! Global game state management and utilities.
//!
//! Contains static variables and helper functions for tracking game state
//! during netplay and replay modes.

use std::sync::atomic::{AtomicI32, AtomicU8};
use std::sync::atomic::Ordering::Relaxed;

use winapi::shared::d3d9types::D3DCOLOR;

use crate::INPUT_KEYS_NUMBERS;

// Battle state
pub static mut BATTLE_STARTED: bool = false;
pub static mut GIRLS_ARE_TALKING: bool = false;
pub static mut GIRLSTALKED: bool = false;
pub static mut HAS_LOADED: bool = false;
pub static mut AFTER_GAME_REQUEST_FROM_P1: bool = false;

// ESC handling
pub static mut ESC: u8 = 0;
pub static ESC2: AtomicU8 = AtomicU8::new(0);

// Input state
pub static mut REAL_INPUT: Option<[bool; INPUT_KEYS_NUMBERS]> = None;
pub static mut REAL_INPUT2: Option<[bool; INPUT_KEYS_NUMBERS]> = None;
pub static mut IS_FIRST_READ_INPUTS: bool = true;

// Network state
pub static DISABLE_SEND: AtomicU8 = AtomicU8::new(0);
pub static LAST_STATE: AtomicU8 = AtomicU8::new(0x6b);
pub static TARGET_OFFSET: AtomicI32 = AtomicI32::new(0);
pub static mut LIKELY_DESYNCED: bool = false;

// Configuration keybindings
pub static mut INCREASE_DELAY_KEY: u8 = 0;
pub static mut DECREASE_DELAY_KEY: u8 = 0;
pub static mut INCREASE_MAX_ROLLBACK_KEY: u8 = 0;
pub static mut DECREASE_MAX_ROLLBACK_KEY: u8 = 0;
pub static mut TOGGLE_STAT_KEY: u8 = 0;
pub static mut TAKEOVER_KEYS_SCHEME: [u8; 4] = [0, 0, 0, 0];

// Delay settings
pub static mut LAST_DELAY_VALUE: usize = 0;
pub static mut DEFAULT_DELAY_VALUE: usize = 0;
pub static mut LAST_DELAY_VALUE_TAKEOVER: usize = 0;
pub static mut AUTODELAY_ENABLED: bool = false;
pub static mut AUTODELAY_ROLLBACK: i8 = 0;
pub static mut LAST_DELAY_MANIP: u8 = 0;

// Display state
pub static mut TOGGLE_STAT: bool = false;
pub static mut LAST_TOGGLE: bool = false;
pub static mut NEXT_DRAW_PING: Option<i32> = None;
pub static mut NEXT_DRAW_ROLLBACK: Option<i32> = None;
pub static mut NEXT_DRAW_ENEMY_DELAY: Option<i32> = None;

// Warning indicators
pub static mut WARNING_FRAME_MISSING_1_COUNTDOWN: usize = 0;
pub static mut WARNING_FRAME_MISSING_2_COUNTDOWN: usize = 0;
pub static mut WARNING_WHEN_LAGGING: bool = true;

// Sound state
pub static mut DISABLE_SOUND: bool = false;
pub static mut FORCE_SOUND_SKIP: bool = false;

// Misc configuration
pub static mut FREEZE_MITIGATION: bool = false;
pub static mut ENABLE_CHECK_MODE: bool = false;
pub static mut MAX_ROLLBACK_PREFERENCE: u8 = 6;

// UI Colors
pub static mut OUTER_COLOR: D3DCOLOR = 0;
pub static mut INSIDE_COLOR: D3DCOLOR = 0;
pub static mut PROGRESS_COLOR: D3DCOLOR = 0;
pub static mut TAKEOVER_COLOR: D3DCOLOR = 0;

// Progress bar positioning
pub static mut CENTER_X_P1: i32 = 224;
pub static mut CENTER_Y_P1: i32 = 428;
pub static mut CENTER_X_P2: i32 = 640 - 224;
pub static mut CENTER_Y_P2: i32 = 428;
pub static mut INSIDE_HALF_HEIGHT: i32 = 7;
pub static mut INSIDE_HALF_WIDTH: i32 = 58;
pub static mut OUTER_HALF_HEIGHT: i32 = 9;
pub static mut OUTER_HALF_WIDTH: i32 = 60;

// Packet caching for freeze mitigation
pub static mut LAST_GAME_REQUEST: Option<[u8; 400]> = None;
pub static mut LAST_LOAD_ACK: Option<[u8; 400]> = None;
pub static mut LAST_MATCH_ACK: Option<[u8; 400]> = None;
pub static mut LAST_MATCH_LOAD: Option<[u8; 400]> = None;

/// Check if the local player is P1 (host).
#[inline]
pub fn is_p1() -> bool {
    unsafe {
        let netmanager = *(0x8986a0 as *const usize);
        *(netmanager as *const usize) == 0x858cac
    }
}

/// Pause the battle state, preserving the previous state for resume.
pub fn pause(battle_state: &mut u32, state_sub_count: &mut u32) {
    if *battle_state != 4 {
        LAST_STATE.store(*battle_state as u8, Relaxed);
        *state_sub_count = state_sub_count.wrapping_sub(1);
        *battle_state = 4;
    }
}

/// Resume the battle from paused state.
pub fn resume(battle_state: &mut u32) {
    let last = LAST_STATE.load(Relaxed);
    if last != 0x6b && *battle_state == 4 {
        *battle_state = last as u32;
        LAST_STATE.store(0x6b, Relaxed)
    }
}

/// Set the input buffers for both players.
pub unsafe fn set_input_buffer(input: [bool; INPUT_KEYS_NUMBERS], input2: [bool; INPUT_KEYS_NUMBERS]) {
    REAL_INPUT = Some(input);
    REAL_INPUT2 = Some(input2);
}

/// Update delay value based on key presses.
pub unsafe fn change_delay_from_keys(ori: usize) -> usize {
    use crate::input::read_key_better;

    let k_up = read_key_better(INCREASE_DELAY_KEY);
    let k_down = read_key_better(DECREASE_DELAY_KEY);

    let last_up = LAST_DELAY_MANIP & 1 == 1;
    let last_down = LAST_DELAY_MANIP & 2 == 2;
    LAST_DELAY_MANIP = k_up as u8 + k_down as u8 * 2;

    if !last_up && k_up {
        ori.saturating_add(1).clamp(0, 9)
    } else if !last_down && k_down {
        ori.saturating_sub(1)
    } else {
        ori
    }
}

/// Update statistics toggle based on key press.
pub unsafe fn update_toggle_stat_from_keys() {
    use crate::input::read_key_better;

    let stat_toggle = read_key_better(TOGGLE_STAT_KEY);
    if stat_toggle && !LAST_TOGGLE {
        TOGGLE_STAT = !TOGGLE_STAT;
    }
    LAST_TOGGLE = stat_toggle;
}

/// Reset game state when exiting a match.
pub unsafe fn reset_match_state() {
    HAS_LOADED = false;
    AFTER_GAME_REQUEST_FROM_P1 = false;
    GIRLS_ARE_TALKING = false;
    LAST_LOAD_ACK = None;
    LAST_GAME_REQUEST = None;
    LAST_MATCH_ACK = None;
    LAST_MATCH_LOAD = None;
    LIKELY_DESYNCED = false;
    NEXT_DRAW_PING = None;
    ESC = 0;
    ESC2.store(0, Relaxed);
    BATTLE_STARTED = false;
    DISABLE_SEND.store(0, Relaxed);
    LAST_STATE.store(0, Relaxed);
    GIRLSTALKED = false;
    NEXT_DRAW_ROLLBACK = None;
    NEXT_DRAW_ENEMY_DELAY = None;
    WARNING_FRAME_MISSING_1_COUNTDOWN = 0;
    WARNING_FRAME_MISSING_2_COUNTDOWN = 0;
}
