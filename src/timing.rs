//! Frame timing and synchronization.
//!
//! Handles frame pacing with optional spin-wait for sub-millisecond precision.

use std::sync::atomic::Ordering::Relaxed;
use std::sync::Mutex;
use std::time::Instant;

use windows::Win32::Foundation::HANDLE;
use windows::Win32::System::Threading::WaitForSingleObject;

#[cfg(feature = "logtofile")]
use log::info;

use crate::println;

pub static mut UPDATE: Option<Instant> = None;
pub static mut TARGET: Option<u128> = None;
pub static mut SPIN_TIME_MICROSECOND: i128 = 0;
pub static mut F62_ENABLED: bool = false;
pub static SOKU_LOOP_EVENT: Mutex<Option<isize>> = Mutex::new(None);
pub static mut WARNING_FRAME_LOST_COUNTDOWN: std::sync::atomic::AtomicU32 =
    std::sync::atomic::AtomicU32::new(0);

pub const VERSION_BYTE_60: u8 = 0x6b;
pub const VERSION_BYTE_62: u8 = 0x6c;

/// Get target frame time in microseconds based on F62 mode.
#[inline]
pub fn target_frametime() -> i32 {
    unsafe {
        if F62_ENABLED {
            1_000_000 / 62
        } else {
            1_000_000 / 60
        }
    }
}

/// Main timing loop callback - handles frame pacing with spin-wait.
pub unsafe extern "cdecl" fn timing_loop(
    a: *mut ilhook::x86::Registers,
    _b: usize,
    _c: usize,
) {
    let target_frametime = target_frametime();
    let waithandle = (*a).esi;

    let (m, target) = match UPDATE {
        Some(x) => (x, TARGET.as_mut().unwrap()),
        None => {
            let m = Instant::now();
            UPDATE = Some(m);
            TARGET = Some(0);
            (m, TARGET.as_mut().unwrap())
        }
    };

    let s = crate::TARGET_OFFSET.swap(0, Relaxed).clamp(-1000, 10000);
    *target += (target_frametime + s) as u128;

    let cur = m.elapsed().as_micros();
    let diff = (*target as i128 + 1000) - cur as i128 - SPIN_TIME_MICROSECOND;

    let ddiff = (diff / 1000) as i32;
    if ddiff < 0 {
        println!("frameskip");
        #[cfg(feature = "logtofile")]
        info!("frameskip {diff}");
        if ddiff > 2 {
            *target = cur + (target_frametime) as u128;
        }
        WARNING_FRAME_LOST_COUNTDOWN.store(115, Relaxed);
    } else {
        WaitForSingleObject(HANDLE(waithandle as isize), ddiff as u32);
        if SPIN_TIME_MICROSECOND != 0 {
            loop {
                let r1 = m.elapsed().as_micros();
                if r1 >= *target {
                    break;
                }
            }
        }
        if let Ok(event) = SOKU_LOOP_EVENT.lock() {
            if let Some(event) = *event {
                if WaitForSingleObject(HANDLE(event), 0).0 == 0 {
                    println!("frame costed too much time!");
                    WARNING_FRAME_LOST_COUNTDOWN.store(115, Relaxed);
                } else if WARNING_FRAME_LOST_COUNTDOWN.load(Relaxed) != 0 {
                    WARNING_FRAME_LOST_COUNTDOWN.fetch_sub(1, Relaxed);
                }
            } else if WARNING_FRAME_LOST_COUNTDOWN.load(Relaxed) != 0 {
                WARNING_FRAME_LOST_COUNTDOWN.fetch_sub(1, Relaxed);
            }
        }
    }
}

pub static mut ORI_CLOSE_LOOP_EVENT: Option<unsafe extern "C" fn(isize) -> isize> = None;
pub static mut ORI_CREATE_LOOP_EVENT: Option<unsafe extern "C" fn() -> isize> = None;

pub unsafe extern "C" fn close_loop_event_override(handle: isize) -> isize {
    if let Ok(mut event) = SOKU_LOOP_EVENT.lock() {
        *event = None;
    }
    ORI_CLOSE_LOOP_EVENT.unwrap()(handle)
}

pub unsafe extern "C" fn create_loop_event_override() -> isize {
    let handle = ORI_CREATE_LOOP_EVENT.unwrap()();
    if let Ok(mut event) = SOKU_LOOP_EVENT.lock() {
        *event = Some(handle);
    }
    handle
}
