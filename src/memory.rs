//! Heap memory management for rollback support.
//!
//! This module intercepts Soku's heap allocations to track memory changes
//! during gameplay, enabling proper state restoration during rollback.

use std::ffi::c_void;
use std::ptr::null_mut;
use std::sync::atomic::Ordering::Relaxed;

use windows::Win32::Foundation::{GetLastError, HANDLE};
use windows::Win32::System::Memory::{HeapAlloc, HeapFree, HEAP_FLAGS};
use windows::Win32::System::Threading::GetCurrentThreadId;

#[cfg(feature = "fillfree")]
use windows::Win32::System::Memory::HEAP_ZERO_MEMORY;

use crate::{REQUESTED_THREAD_ID, SOKU_FRAMECOUNT};

pub static mut MEMORY_SENDER_FREE: Option<std::sync::mpsc::Sender<usize>> = None;
pub static mut MEMORY_RECEIVER_FREE: Option<std::sync::mpsc::Receiver<usize>> = None;

pub static mut MEMORY_SENDER_ALLOC: Option<std::sync::mpsc::Sender<usize>> = None;
pub static mut MEMORY_RECEIVER_ALLOC: Option<std::sync::mpsc::Receiver<usize>> = None;

pub static mut ORI_HEAP_REALLOC: Option<unsafe extern "stdcall" fn(isize, u32, usize, usize) -> usize> = None;

#[cfg(feature = "fillfree")]
static mut HEAP_FREE_RNG: Option<rand::rngs::ThreadRng> = None;

#[cfg(feature = "fillfree")]
pub unsafe fn fill_random(addr: usize, size: Option<usize>) {
    use crate::rollback::read_heap;
    let size = size.or_else(|| Some(read_heap(addr))).unwrap();
    let a = std::slice::from_raw_parts_mut(addr as *mut u8, size);
    use rand::{thread_rng, Rng};
    if HEAP_FREE_RNG.is_none() {
        HEAP_FREE_RNG = Some(thread_rng());
    }
    let rng = HEAP_FREE_RNG.as_mut().unwrap();
    for byte in a {
        // (3/4)^4 is approximately equal to 0.32.
        // The possibility that a specific int32_t will be filled zero will be approximately equal to 0.32.
        if rng.gen_ratio(3, 4) {
            *byte = 0;
        } else {
            *byte = rng.gen();
        }
    }
}

#[macro_export]
macro_rules! soku_heap_free {
    ($ptr:expr) => {{
        use std::ffi::c_void;
        use windows::Win32::{
            Foundation::HANDLE,
            System::Memory::{HeapFree, HEAP_FLAGS},
        };
        let a: usize = $ptr;
        #[cfg(feature = "fillfree")]
        {
            use crate::memory::fill_random;
            fill_random(a, None);
        }
        HeapFree(
            HANDLE(*(0x89b404 as *const isize)),
            HEAP_FLAGS(0),
            Some(a as *const c_void),
        )
        .unwrap_or_else(|e| panic!("HeapFree failed for {:?}", e));
    }};
}

pub unsafe extern "stdcall" fn heap_free_override(heap: isize, flags: u32, s: *const c_void) -> i32 {
    if *(0x89b404 as *const isize) != heap
        || GetCurrentThreadId() != REQUESTED_THREAD_ID.load(Relaxed)
        || *SOKU_FRAMECOUNT == 0
    {
        return HeapFree(HANDLE(heap), HEAP_FLAGS(flags), Some(s)).is_ok() as i32;
    }

    MEMORY_SENDER_FREE
        .as_ref()
        .unwrap()
        .clone()
        .send(s as usize)
        .unwrap();

    1
}

pub unsafe extern "stdcall" fn heap_alloc_override(heap: isize, flags: u32, s: usize) -> *mut c_void {
    let ret = HeapAlloc(HANDLE(heap), HEAP_FLAGS(flags), s);

    if *(0x89b404 as *const usize) != heap as usize
        || *SOKU_FRAMECOUNT == 0
        || GetCurrentThreadId() != REQUESTED_THREAD_ID.load(Relaxed)
    {
        // Wrong heap or not in battle
    } else {
        assert_ne!(ret, null_mut(), "HeapAlloc failed for {:?}", GetLastError());
        #[cfg(feature = "fillfree")]
        if flags & HEAP_ZERO_MEMORY.0 == 0 {
            fill_random(ret as usize, Some(s));
        }
        store_alloc(ret as usize);
    }
    ret
}

pub unsafe extern "stdcall" fn heap_realloc_override(
    heap: isize,
    flags: u32,
    p: usize,
    s: usize,
) -> usize {
    if *(0x89b404 as *const usize) != heap as usize
        || *SOKU_FRAMECOUNT == 0
        || GetCurrentThreadId() != REQUESTED_THREAD_ID.load(Relaxed)
    {
        ORI_HEAP_REALLOC.unwrap()(heap, flags, p, s)
    } else {
        REQUESTED_THREAD_ID.store(0, Relaxed);
        panic!("HeapRealloc({},{},{},{})!!!", heap, flags, p, s);
    }
}

fn store_alloc(u: usize) {
    unsafe {
        MEMORY_SENDER_ALLOC
            .as_ref()
            .unwrap()
            .clone()
            .send(u)
            .unwrap();
    }
}

/// Initialize memory channels for rollback tracking.
pub fn init_channels() {
    unsafe {
        let (s, r) = std::sync::mpsc::channel();
        MEMORY_RECEIVER_FREE = Some(r);
        MEMORY_SENDER_FREE = Some(s);

        let (s, r) = std::sync::mpsc::channel();
        MEMORY_RECEIVER_ALLOC = Some(r);
        MEMORY_SENDER_ALLOC = Some(s);
    }
}
