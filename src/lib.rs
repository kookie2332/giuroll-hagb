#![feature(coroutines)]
#![feature(iter_from_coroutine)]
#![feature(anonymous_lifetime_in_impl_trait)]
#![feature(panic_update_hook)]
#![feature(stmt_expr_attributes)]
#![allow(static_mut_refs)]

use std::ffi::c_void;
use std::os::windows::ffi::OsStringExt;
use std::panic;
use std::path::{Path, PathBuf};
use std::ptr::addr_of_mut;
use std::sync::atomic::{AtomicU32, Ordering::Relaxed};
use std::sync::Mutex;
use std::time::{Duration, Instant};

mod camera;
mod config;
mod game_state;
mod input;
mod memory;
mod netcode;
mod replay;
mod rollback;
mod sound;
mod timing;
mod ui;
mod version;

use ilhook::x86::{HookPoint, HookType};

#[cfg(feature = "logtofile")]
use log::info;
use windows::core::PCWSTR;
use windows::Win32::Foundation::{HMODULE, HWND};
use windows::Win32::Networking::WinSock::{closesocket, SOCKADDR, SOCKET};
use windows::Win32::System::LibraryLoader::GetModuleFileNameW;
use windows::Win32::System::Memory::{VirtualProtect, PAGE_EXECUTE_READWRITE, PAGE_PROTECTION_FLAGS};
use windows::Win32::System::Threading::GetCurrentThreadId;

use camera::{
    apply_config as apply_camera_config, cbattle_process_smooth, clear_smoothed_transform,
    enable_runtime_smoothing, reset_state as reset_camera_state, save_last_transform,
    smoothing_allowed,
};
use config::GiurollConfig;
use game_state::*;
use input::{input_to_accum, read_current_input, read_key_better};
use memory::{heap_alloc_override, heap_free_override, heap_realloc_override, MEMORY_RECEIVER_ALLOC, MEMORY_RECEIVER_FREE, ORI_HEAP_REALLOC};
use netcode::{send_packet_untagged, NetworkPacket, Netcoder};
use replay::{apause, clean_replay_statics, handle_replay, is_replay_over, render_replay_progress_bar_and_numbers};
use rollback::{Rollbacker, CHARSIZEDATA, DUMP_FRAME_TIME, LAST_M_LEN, MEMORY_LEAK};
use sound::RollbackSoundManager;
use timing::{
    close_loop_event_override, create_loop_event_override, timing_loop,
    F62_ENABLED, ORI_CLOSE_LOOP_EVENT, ORI_CREATE_LOOP_EVENT,
    VERSION_BYTE_60, VERSION_BYTE_62, WARNING_FRAME_LOST_COUNTDOWN,
};
use ui::{draw_num, draw_num_x_center, get_num_length};

pub use version::{compareVersion, compareVersionString, getPriority, getVersion, getVersionString, VERSION_STR};

const ISDEBUG: bool = false;
pub(crate) const INPUT_KEYS_NUMBERS: usize = 12;
pub(crate) const SOKU_FRAMECOUNT: *mut usize = 0x8985d8 as *mut usize;

static HOOK: Mutex<Option<Box<[HookPoint]>>> = Mutex::new(None);
pub(crate) static REQUESTED_THREAD_ID: AtomicU32 = AtomicU32::new(0);

static mut ENABLE_PRINTLN: bool = false;

#[macro_export]
macro_rules! println {
    ($($arg:tt)*) => {{
        use crate::ENABLE_PRINTLN;
        use crate::replay::CHECK;
        #[allow(unused_unsafe)]
        if unsafe { ENABLE_PRINTLN } || unsafe { CHECK.is_some() } {
            std::println!($($arg)*);
        }
    }};
}

#[macro_export]
macro_rules! ptr_wrap {
    ($src:expr) => {{ $src }};
}

// Callbacks for external mods
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct Callbacks {
    pub save_state: unsafe extern "C" fn() -> u32,
    pub load_state_pre: unsafe extern "C" fn(usize, u32),
    pub load_state_post: unsafe extern "C" fn(u32),
    pub free_state: unsafe extern "C" fn(u32, bool),
}

pub(crate) static mut CALLBACK_ARRAY: Vec<Callbacks> = Vec::new();

#[no_mangle]
pub unsafe extern "C" fn addRollbackCb(cb: *const Callbacks) {
    CALLBACK_ARRAY.push(*cb);
}

// Sound management
static mut SOUND_MANAGER: Option<RollbackSoundManager> = None;

pub fn force_sound_skip(soundid: usize) {
    unsafe {
        let forcesound = std::mem::transmute::<usize, extern "stdcall" fn(u32)>(0x401d50);
        FORCE_SOUND_SKIP = true;
        forcesound(soundid as u32);
        FORCE_SOUND_SKIP = false;
    }
}

// Network state
static mut ROLLBACKER: Option<Rollbacker> = None;
static mut NETCODER: Option<Netcoder> = None;
static mut DATA_SENDER: Option<std::sync::mpsc::Sender<(NetworkPacket, Instant)>> = None;
static mut DATA_RECEIVER: Option<std::sync::mpsc::Receiver<(NetworkPacket, Instant)>> = None;

// TSK compatibility
#[repr(C)]
struct FakeBattleManagerForTsk {
    fake_left_win_count: u8,
    fake_right_win_count: u8,
    _unused1: [u8; 0xa],
    fake_p_left_char: *mut u8,
    fake_p_right_char: *mut u8,
    _unused2: [u8; 0x74],
    fake_battle_mode: u32,
}

const P_FAKE_BATTLE_MANAGER_FOR_TSK: *mut *mut FakeBattleManagerForTsk = 0x47579c as _;
const P_VER_BYTE: *mut u8 = 0x475798 as _;
static mut FAKE_BATTLE_MANAGER_FOR_TSK: Option<Box<FakeBattleManagerForTsk>> = None;

impl FakeBattleManagerForTsk {
    fn new_box() -> Box<Self> {
        let mut self_ = Box::new(Self {
            fake_battle_mode: 0,
            fake_p_left_char: std::ptr::null_mut(),
            fake_p_right_char: std::ptr::null_mut(),
            fake_left_win_count: 0,
            fake_right_win_count: 0,
            _unused1: [0; 0xa],
            _unused2: [0; 0x74],
        });
        self_.fake_p_left_char = (addr_of_mut!(self_.fake_left_win_count) as usize).wrapping_sub(0x573) as _;
        self_.fake_p_right_char = (addr_of_mut!(self_.fake_right_win_count) as usize).wrapping_sub(0x573) as _;
        self_
    }
}

// Receive packet handling for select screen latency display
static mut ORI_RECVFROM: Option<unsafe extern "stdcall" fn(SOCKET, *mut u8, i32, i32, *mut SOCKADDR, *mut i32) -> u32> = None;

struct P2SendTimeData {
    has_received: bool,
    last_frame_id: usize,
    last_receive_time: Instant,
    last_max_latency: Option<Duration>,
    max_latency_to_be_shown: Option<Duration>,
    last_shown_frame: usize,
}

static SELECT_SCENE_INPUT_SEND_TIME_DATA: Mutex<Option<P2SendTimeData>> = Mutex::new(None);

// ===== Utility Functions =====

fn warning_box(text: &str, title: &str) {
    let to_utf_16 = |s: &str| s.encode_utf16().chain([0]).collect::<Vec<u16>>();
    unsafe {
        use windows::Win32::UI::WindowsAndMessaging::{MessageBoxW, MB_ICONERROR, MB_OK};
        MessageBoxW(
            HWND(0),
            PCWSTR(to_utf_16(text).as_ptr()),
            PCWSTR(to_utf_16(title).as_ptr()),
            MB_OK | MB_ICONERROR,
        );
    }
}

unsafe fn tamper_memory<T: Sized>(dst: *mut T, src: T) -> T {
    let mut old_prot_ptr = PAGE_PROTECTION_FLAGS(0);
    VirtualProtect(dst as _, std::mem::size_of::<T>(), PAGE_EXECUTE_READWRITE, &mut old_prot_ptr).unwrap();
    let ori = dst.read_unaligned();
    dst.write_unaligned(src);
    VirtualProtect(dst as _, std::mem::size_of::<T>(), old_prot_ptr, &mut old_prot_ptr).unwrap();
    ori
}

unsafe fn jmp_relative_opt_to_pointer<T: Sized>(jmp_addr: *const c_void) -> T {
    let p_offset = (jmp_addr as *mut u8).offset(1) as *mut usize;
    let end = (jmp_addr as usize).wrapping_add(1 + std::mem::size_of::<usize>());
    let ret = end.wrapping_add(p_offset.read_unaligned());
    std::mem::transmute_copy(&ret)
}

unsafe fn tamper_jmp_relative_opr<T: Sized>(dst: *mut c_void, src: T) -> T {
    let mut old_prot_ptr = PAGE_PROTECTION_FLAGS(0);
    let p_offset = (dst as *mut u8).offset(1) as *mut usize;
    let end = (dst as usize).wrapping_add(1 + std::mem::size_of::<usize>());
    let ret = jmp_relative_opt_to_pointer(dst);
    VirtualProtect(p_offset as _, std::mem::size_of::<usize>(), PAGE_EXECUTE_READWRITE, &mut old_prot_ptr).unwrap();
    p_offset.write_unaligned((std::mem::transmute_copy::<T, usize>(&src)).wrapping_sub(end));
    VirtualProtect(p_offset as _, std::mem::size_of::<usize>(), old_prot_ptr, &mut old_prot_ptr).unwrap();
    ret
}

// ===== DLL Entry Points =====

#[no_mangle]
pub extern "C" fn InitializeByLoader(dllmodule: HMODULE) -> bool {
    initialize(dllmodule, true)
}

#[no_mangle]
pub extern "C" fn Initialize(dllmodule: HMODULE) -> bool {
    initialize(dllmodule, false)
}

fn initialize(dllmodule: HMODULE, pretend_to_be_vanilla: bool) -> bool {
    let mut dat = [0u16; 1025];
    unsafe { GetModuleFileNameW(dllmodule, &mut dat) };
    let s = std::ffi::OsString::from_wide(&dat);
    let mut filepath = Path::new(&s).to_owned();
    filepath.pop();

    static GR_LOADED: Mutex<bool> = Mutex::new(false);
    let mut lock = GR_LOADED.lock().unwrap();

    if *lock {
        warning_box("This giuroll had been initialized! Please don't initialize it again!", "Failed to initialize Giuroll");
        return false;
    }
    match truer_exec(filepath, pretend_to_be_vanilla) {
        Ok(_) => { *lock = true; true }
        Err(e) => { warning_box(&e, "Failed to initialize Giuroll"); false }
    }
}

#[no_mangle]
pub extern "cdecl" fn CheckVersion(a: *const [u8; 16]) -> bool {
    const HASH110A: [u8; 16] = [0xdf, 0x35, 0xd1, 0xfb, 0xc7, 0xb5, 0x83, 0x31, 0x7a, 0xda, 0xbe, 0x8c, 0xd9, 0xf5, 0x3b, 0x2e];
    unsafe { *ptr_wrap!(a) == HASH110A }
}

#[no_mangle]
pub extern "cdecl" fn cleanup() {
    HOOK.lock().unwrap().take().unwrap().into_vec().into_iter().for_each(|x| unsafe { x.unhook() });
}

#[no_mangle]
pub extern "cdecl" fn is_likely_desynced() -> bool {
    unsafe { LIKELY_DESYNCED }
}

#[cfg(feature = "logtofile")]
pub fn set_up_fern() -> Result<(), fern::InitError> {
    fern::Dispatch::new()
        .format(|out, message, record| {
            out.finish(format_args!("[{} {}] {}", record.level(), record.target(), message))
        })
        .level(log::LevelFilter::Debug)
        .chain(fern::log_file("output.log")?)
        .apply()?;
    Ok(())
}

// ===== Main Initialization =====

fn truer_exec(filename: PathBuf, pretend_to_be_vanilla: bool) -> Result<(), String> {
    panic::update_hook(|prev, info| {
        warning_box(
            &format!("{}\n{}\n{info:}",
                if cfg!(feature = "cn") { "Giuroll detected an exception!" } else { "Giuroll was panicked, which may or may not be caused by Giuroll." },
                if cfg!(feature = "cn") { "Your feedback is important!" } else { "Please take a screenshot and report to @hagb_ in hisoutensoku Discord." }
            ),
            "Panic!",
        );
        prev(info);
    });

    #[cfg(feature = "allocconsole")]
    unsafe { windows::Win32::System::Console::AllocConsole(); }

    unsafe {
        if *(0x8A0040 as *const usize) >= 5 || *(0x8A0044 as *const usize) >= 5 {
            return Err("Please don't load Giuroll in battle".to_string());
        }

        let netcode_mod = match *(0x858b80 as *const u8) {
            0x69 | 0x6a => Some("Giuroll < 0.6"),
            VERSION_BYTE_60 | VERSION_BYTE_62 => Some("Giuroll 0.6.x"),
            0x64 => Some("SokuRoll"),
            0x6e => None,
            _ => match *P_VER_BYTE { 0xcc => None, _ => Some("unknown netcode mod") },
        };
        if let Some(netcode_mod) = netcode_mod {
            return Err(format!("Conflict! An other netcode mod ({}) had been loaded!", netcode_mod));
        }
    }

    let mut filepath = filename;
    filepath.push("giuroll.ini");
    let config = GiurollConfig::from_file(filepath, pretend_to_be_vanilla)?;

    #[cfg(feature = "logtofile")]
    { set_up_fern().unwrap(); }

    // Initialize channels
    unsafe {
        let (s, r) = std::sync::mpsc::channel();
        DATA_RECEIVER = Some(r);
        DATA_SENDER = Some(s);
    }
    memory::init_channels();

    let turning_off_all_extra_ui = config.turning_off_all_extra_ui;
    apply_config(&config);

    let title = config.title.clone();
    setup_hooks(&config, turning_off_all_extra_ui)?;
    spawn_title_thread(title);

    Ok(())
}

fn apply_config(config: &GiurollConfig) {
    unsafe {
        // Soku2 character data compatibility
        init_char_size_data(config.soku2_compatibility_mode);

        timing::F62_ENABLED = config.enable_f62;
        timing::SPIN_TIME_MICROSECOND = config.spin_amount as i128;

        INCREASE_DELAY_KEY = config.increase_delay_key as u8;
        DECREASE_DELAY_KEY = config.decrease_delay_key as u8;
        INCREASE_MAX_ROLLBACK_KEY = config.increase_max_rollback_key as u8;
        DECREASE_MAX_ROLLBACK_KEY = config.decrease_max_rollback_key as u8;
        TOGGLE_STAT_KEY = config.toggle_network_stats as u8;
        TAKEOVER_KEYS_SCHEME = [
            config.exit_takeover as u8,
            config.p1_takeover as u8,
            config.p2_takeover as u8,
            config.set_or_retry_takeover as u8,
        ];

        TOGGLE_STAT = config.enable_network_stats_by_default;
        LAST_DELAY_VALUE = config.default_delay as usize;
        DEFAULT_DELAY_VALUE = config.default_delay as usize;
        AUTODELAY_ENABLED = config.auto_delay_enabled;
        AUTODELAY_ROLLBACK = config.auto_delay_rollback as i8;
        LAST_DELAY_VALUE_TAKEOVER = config.default_delay_takeover as usize;

        OUTER_COLOR = config.outer_color;
        INSIDE_COLOR = config.inside_color;
        PROGRESS_COLOR = config.progress_color;
        TAKEOVER_COLOR = config.takeover_color;
        CENTER_X_P1 = config.center_x_p1 as i32;
        CENTER_X_P2 = config.center_x_p2 as i32;
        CENTER_Y_P1 = config.center_y_p1 as i32;
        CENTER_Y_P2 = config.center_y_p2 as i32;
        INSIDE_HALF_HEIGHT = config.inside_half_height as i32;
        INSIDE_HALF_WIDTH = config.inside_half_width as i32;
        OUTER_HALF_HEIGHT = config.outer_half_height as i32;
        OUTER_HALF_WIDTH = config.outer_half_width as i32;

        FREEZE_MITIGATION = config.freeze_mitigation;
        ENABLE_PRINTLN = config.enable_println;
        ENABLE_CHECK_MODE = config.enable_check_mode;
        WARNING_WHEN_LAGGING = config.warning_when_lagging;
        MAX_ROLLBACK_PREFERENCE = config.max_rollback_preference;

        apply_camera_config(config);
    }
}

unsafe fn init_char_size_data(soku2_mode: bool) {
    let (data_a, data_b): (&[usize], &[usize]) = if soku2_mode {
        (&[2236, 2220, 2208, 2244, 2216, 2284, 2196, 2220, 2260, 2200, 2232, 2200, 2200, 2216,
           2352, 2224, 2196, 2196, 2216, 2216, 0, 2208, 2236, 2232, 2196, 2196, 2216, 2216,
           2200, 2216, 2352, 2200, 2284, 2220, 2208],
         &[940, 940, 940, 944, 940, 940, 940, 940, 940, 940, 940, 940, 940, 940, 940, 940,
           940, 940, 940, 940, 0, 940, 940, 940, 940, 940, 940, 940, 940, 940, 940, 940, 940,
           940, 940])
    } else {
        (&[2236, 2220, 2208, 2244, 2216, 2284, 2196, 2220, 2260, 2200, 2232, 2200, 2200, 2216,
           2352, 2224, 2196, 2196, 2216, 2216],
         &[940, 940, 940, 944, 940, 940, 940, 940, 940, 940, 940, 940, 940, 940, 940, 940,
           940, 940, 940, 940])
    };

    for i in 0..data_a.len() {
        if CHARSIZEDATA.len() == i { CHARSIZEDATA.push((0, 0)); }
        if CHARSIZEDATA[i] == (0, 0) { CHARSIZEDATA[i] = (data_a[i], data_b[i]); }
    }
}

fn setup_hooks(config: &GiurollConfig, turning_off_all_extra_ui: bool) -> Result<(), String> {
    unsafe {
        // TSK compatibility
        FAKE_BATTLE_MANAGER_FOR_TSK = Some(FakeBattleManagerForTsk::new_box());
        tamper_memory(P_FAKE_BATTLE_MANAGER_FOR_TSK, FAKE_BATTLE_MANAGER_FOR_TSK.as_mut().unwrap().as_mut() as _);

        // Version byte
        let ver_byte = if F62_ENABLED { VERSION_BYTE_62 } else { VERSION_BYTE_60 };
        tamper_memory(0x858b80 as *mut u8, ver_byte);
        tamper_memory(P_VER_BYTE, ver_byte);

        // Meiling d236 desync fix
        tamper_memory(0x724316 as *mut [u8; 4], [0x66, 0xB9, 0x0F, 0x00]);

        // 9 digit font fix
        for a in [0x43DC7D, 0x882954] {
            tamper_memory(a as *mut u8, 0x0A);
        }

        // ChainCFix
        setup_chaincfix();

        // Main hook
        std::mem::forget(ilhook::x86::Hooker::new(0x482701, HookType::JmpBack(main_hook), 0).hook(6));

        // Sound hooks
        setup_sound_hooks();

        // Exit hook
        std::mem::forget(ilhook::x86::Hooker::new(0x481960, HookType::JmpBack(on_exit), 0).hook(6));

        // Spectator skip hook
        setup_spectator_hook();

        // Camera smoothing hooks
        setup_camera_hooks();

        // UI rendering hooks
        if !turning_off_all_extra_ui {
            std::mem::forget(ilhook::x86::Hooker::new(0x43e320, HookType::JmpBack(drawnumbers), 0).hook(7));
            std::mem::forget(ilhook::x86::Hooker::new(0x42158f, HookType::JmpBack(render_number_on_select), 0).hook(5));
        }

        // Character select hooks
        setup_select_hooks();

        // Girls talking hook
        std::mem::forget(ilhook::x86::Hooker::new(0x482960, HookType::JmpBack(ongirlstalk), 0).hook(5));

        // Memory hooks
        tamper_memory(0x00857170 as _, heap_free_override as unsafe extern "stdcall" fn(_, _, _) -> _);
        tamper_memory(0x00857174 as _, heap_alloc_override as unsafe extern "stdcall" fn(_, _, _) -> _);
        ORI_HEAP_REALLOC = Some(tamper_memory(0x00857180 as _, heap_realloc_override as _));

        // Replay pause hook
        std::mem::forget(ilhook::x86::Hooker::new(0x48267a, HookType::JmpBack(apause), 0).hook(8));

        // Online data hook
        std::mem::forget(ilhook::x86::Hooker::new(0x41daea, HookType::JmpBack(readonlinedata), 0).hook(5));

        // ESC handling hooks
        setup_esc_hooks();

        // Input handling hook
        std::mem::forget(ilhook::x86::Hooker::new(0x46c900, HookType::JmpToAddr(0x46c908, 0, handle_raw_input), 0).hook(8));

        // Timing hooks
        std::mem::forget(ilhook::x86::Hooker::new(0x4192f0, HookType::JmpToAddr(0x4193d7, 0, timing_loop), 0).hook(6));
        ORI_CLOSE_LOOP_EVENT = Some(tamper_jmp_relative_opr(0x408092 as _, close_loop_event_override as _));
        ORI_CREATE_LOOP_EVENT = Some(tamper_jmp_relative_opr(0x407dec as _, create_loop_event_override as _));

        // Packet sniffing hooks
        std::mem::forget(ilhook::x86::Hooker::new(0x4171b4, HookType::JmpBack(sniff_sent), 0).hook(5));
        std::mem::forget(ilhook::x86::Hooker::new(0x4171c7, HookType::JmpBack(sniff_sent), 0).hook(5));

        // Freeze mitigation
        if config.freeze_mitigation {
            tamper_jmp_relative_opr(0x0041dae5 as _, recvfrom_with_fake_packet as unsafe extern "stdcall" fn(_, _, _, _, _, _) -> _);
        }

        // Replay over hook
        tamper_jmp_relative_opr(0x00482689 as _, is_replay_over as unsafe extern "fastcall" fn(_) -> _);
    }

    Ok(())
}

fn spawn_title_thread(title: String) {
    unsafe {
        std::thread::spawn(move || {
            loop {
                std::thread::sleep(Duration::from_millis(3000));
                let hwnd = *(0x89ff90 as *const HWND);
                if hwnd == HWND(0) { continue; }
                let mut origin_title = [0u16; 1024];
                windows::Win32::UI::WindowsAndMessaging::GetWindowTextW(hwnd, &mut origin_title);
                let origin_title_length = origin_title.iter().position(|x| *x == 0).unwrap_or(0);
                let title: Vec<u16> = title.encode_utf16()
                    .flat_map(|x| if x == b'%' as u16 { origin_title[..origin_title_length].iter().copied().collect() } else { vec![x] })
                    .chain([0])
                    .collect();
                let _ = windows::Win32::UI::WindowsAndMessaging::SetWindowTextW(hwnd, PCWSTR::from_raw(title.as_ptr()));
                break;
            }
        });
    }
}

// ===== Hook Setup Helpers =====

unsafe fn setup_chaincfix() {
    unsafe extern "thiscall" fn my_object_handler_spawn_bullet(
        player: usize, action: i32, x: f32, y: f32, dir: i32, color: u32, data: *mut f32, size: usize,
    ) {
        let origin: unsafe extern "thiscall" fn(_, _, _, _, _, _, _, _) = std::mem::transmute(0x46eb30);
        let soku_operator_new: unsafe extern "cdecl" fn(usize) -> *mut u8 = std::mem::transmute(0x0081FBDC);
        let soku_operator_delete: unsafe extern "cdecl" fn(*mut u8) = std::mem::transmute(0x0081F6FA);
        let new_size = size + 1;
        let new_data = soku_operator_new(new_size * std::mem::size_of::<f32>()) as *mut f32;
        new_data.copy_from(data, size);
        *new_data.offset((new_size - 1) as isize) = 1.0;
        origin(player, action, x, y, dir, color, new_data, new_size);
        soku_operator_delete(new_data as _);
    }
    for a in [0x590486, 0x590C4C, 0x590683, 0x5906ED] {
        tamper_jmp_relative_opr(a as _, my_object_handler_spawn_bullet as unsafe extern "thiscall" fn(_, _, _, _, _, _, _, _));
    }
}

unsafe fn setup_sound_hooks() {
    let handle_sound_real_ret = vec![0x401d58, 0x401db7];
    unsafe extern "cdecl" fn handle_sound_real(a: *mut ilhook::x86::Registers, _: usize, _: usize) -> usize {
        (*a).ecx = 0x89f9f8;
        (*a).eax = (((*a).esp + 4) as *const u32).read_unaligned();
        let soundid = (*a).eax as usize;
        if DISABLE_SOUND { return 1; }
        if !BATTLE_STARTED || soundid == 0 { return if soundid == 0 { 1 } else { 0 }; }
        if let Some(manager) = SOUND_MANAGER.as_mut() {
            if FORCE_SOUND_SKIP { return 1; }
            if manager.insert_sound(*SOKU_FRAMECOUNT, soundid) { 0 } else { 1 }
        } else { 0 }
    }
    std::mem::forget(ilhook::x86::Hooker::new(0x401d50, HookType::JmpToEnumRet(handle_sound_real_ret, handle_sound_real), 0).hook(6));

    let soundskiphook1_ret = vec![0x401db6, 0x401d8c, 0x401d81];
    unsafe extern "cdecl" fn soundskiphook1(a: *mut ilhook::x86::Registers, _: usize, _: usize) -> usize {
        if FORCE_SOUND_SKIP {
            let eax = *ptr_wrap!(((*a).esi + 4) as *const u32);
            let ecx = *ptr_wrap!(eax as *const u32);
            let fun = *ptr_wrap!((ecx + 0x48) as *const u32);
            let true_fun = std::mem::transmute::<usize, extern "thiscall" fn(u32, u32)>(fun as usize);
            true_fun(ecx, eax);
            0
        } else {
            if ((((*a).esp + 8) as *const usize).read_unaligned() & 1) == 0 { 1 } else { 2 }
        }
    }
    std::mem::forget(ilhook::x86::Hooker::new(0x401d7a, HookType::JmpToEnumRet(soundskiphook1_ret, soundskiphook1), 0).hook(5));

    // No KO sound fix
    unsafe extern "stdcall" fn override_play_sfx(sound_id: u32) {
        let is_story_or_result_mode = matches!(*(0x00898690 as *const u32), 0 | 7);
        if is_story_or_result_mode || sound_id != 0x2c {
            std::mem::transmute::<usize, extern "stdcall" fn(u32)>(0x439490)(sound_id);
        }
    }
    for addr in [0x6d828b, 0x6dcc0f] {
        tamper_jmp_relative_opr(addr as _, override_play_sfx as unsafe extern "stdcall" fn(u32));
    }
}

unsafe fn setup_spectator_hook() {
    let spectator_skip_ret = vec![0x42daac, 0x42db21];
    unsafe extern "cdecl" fn spectator_skip(a: *mut ilhook::x86::Registers, _: usize, _: usize) -> usize {
        let framecount_cur = *ptr_wrap!(((*a).esi + 0x4c) as *const u32);
        let edi = (*a).edi;
        let no_skip = edi + 16 < framecount_cur && BATTLE_STARTED;
        if no_skip {
            (*a).ebx = *ptr_wrap!(((*a).esi + 0x48) as *const u32);
            (*a).ecx = framecount_cur;
            0
        } else {
            (*a).ebx = (((*a).esp + 0x1c) as *const u32).read_unaligned();
            1
        }
    }
    std::mem::forget(ilhook::x86::Hooker::new(0x42daa6, HookType::JmpToEnumRet(spectator_skip_ret, spectator_skip), 0).hook(6));
}

unsafe fn setup_camera_hooks() {
    static mut CBATTLE_PROCESS: Option<unsafe extern "thiscall" fn(usize) -> usize> = None;
    unsafe extern "thiscall" fn cbattle_render(cbattle: usize) -> usize { cbattle_process_smooth(CBATTLE_PROCESS.unwrap(), cbattle) }
    CBATTLE_PROCESS = Some(tamper_memory(0x008574a4 as _, cbattle_render as _));

    static mut CBATTLECL_PROCESS: Option<unsafe extern "thiscall" fn(usize) -> usize> = None;
    unsafe extern "thiscall" fn cbattlecl_render(cbattle: usize) -> usize { cbattle_process_smooth(CBATTLECL_PROCESS.unwrap(), cbattle) }
    CBATTLECL_PROCESS = Some(tamper_memory(0x00857574 as _, cbattlecl_render as _));

    static mut CBATTLESV_PROCESS: Option<unsafe extern "thiscall" fn(usize) -> usize> = None;
    unsafe extern "thiscall" fn cbattlesv_render(cbattle: usize) -> usize { cbattle_process_smooth(CBATTLESV_PROCESS.unwrap(), cbattle) }
    CBATTLESV_PROCESS = Some(tamper_memory(0x0085751c as _, cbattlesv_render as _));

    static mut CBATTLE_WATCH_PROCESS: Option<unsafe extern "thiscall" fn(usize) -> usize> = None;
    unsafe extern "thiscall" fn cbattle_watch_render(cbattle: usize) -> usize { cbattle_process_smooth(CBATTLE_WATCH_PROCESS.unwrap(), cbattle) }
    CBATTLE_WATCH_PROCESS = Some(tamper_memory(0x00857590 as _, cbattle_watch_render as _));

    for i in [0x004295c1, 0x0047fda0, 0x0048075d, 0x004796fe] {
        tamper_jmp_relative_opr(i as _, save_last_transform as unsafe extern "thiscall" fn(usize));
    }
}

unsafe fn setup_select_hooks() {
    static mut ORI_CSELECT_CL_ON_PROCESS: Option<unsafe extern "thiscall" fn(*mut c_void) -> usize> = None;
    static mut ORI_CSELECT_SV_ON_PROCESS: Option<unsafe extern "thiscall" fn(*mut c_void) -> usize> = None;

    unsafe fn my_cselect_on_process(origin: unsafe extern "thiscall" fn(*mut c_void) -> usize, this_: *mut c_void) -> usize {
        static mut MAX_ROLLBACK_KEY_PRESSED: bool = false;
        let ret = origin(this_);
        if !matches!(ret, 8 | 9) {
            NEXT_DRAW_PING = None;
            *SELECT_SCENE_INPUT_SEND_TIME_DATA.lock().unwrap() = None;
            return ret;
        }
        if *((this_ as usize + 0x4f60) as *const i32) >= 1 { return ret; }
        if read_key_better(DECREASE_MAX_ROLLBACK_KEY) {
            if !MAX_ROLLBACK_KEY_PRESSED { MAX_ROLLBACK_PREFERENCE = MAX_ROLLBACK_PREFERENCE.saturating_sub(1).clamp(0, 15); }
            MAX_ROLLBACK_KEY_PRESSED = true;
        } else if read_key_better(INCREASE_MAX_ROLLBACK_KEY) {
            if !MAX_ROLLBACK_KEY_PRESSED { MAX_ROLLBACK_PREFERENCE = MAX_ROLLBACK_PREFERENCE.saturating_add(1).clamp(0, 15); }
            MAX_ROLLBACK_KEY_PRESSED = true;
        } else { MAX_ROLLBACK_KEY_PRESSED = false; }
        let mut time_data = SELECT_SCENE_INPUT_SEND_TIME_DATA.lock().unwrap();
        if time_data.is_none() {
            *time_data = Some(P2SendTimeData { has_received: false, last_max_latency: None, max_latency_to_be_shown: None, last_receive_time: Instant::now(), last_frame_id: 0, last_shown_frame: 0 });
        }
        update_toggle_stat_from_keys();
        ret
    }

    unsafe extern "thiscall" fn my_cselect_cl_on_process(this_: *mut c_void) -> usize { my_cselect_on_process(ORI_CSELECT_CL_ON_PROCESS.unwrap(), this_) }
    unsafe extern "thiscall" fn my_cselect_sv_on_process(this_: *mut c_void) -> usize { my_cselect_on_process(ORI_CSELECT_SV_ON_PROCESS.unwrap(), this_) }

    ORI_CSELECT_SV_ON_PROCESS = Some(tamper_memory(0x8574e0 as _, my_cselect_sv_on_process as _));
    ORI_CSELECT_CL_ON_PROCESS = Some(tamper_memory(0x857538 as _, my_cselect_cl_on_process as _));
}

unsafe fn setup_esc_hooks() {
    unsafe extern "cdecl" fn skip(_: *mut ilhook::x86::Registers, _: usize, _: usize) {}

    std::mem::forget(ilhook::x86::Hooker::new(0x428374, HookType::JmpToAddr(0x42837f, 0, skip), 0).hook(5));
    std::mem::forget(ilhook::x86::Hooker::new(0x428644, HookType::JmpToAddr(0x42864f, 0, skip), 0).hook(5));

    let skiponcehost_ret = vec![0x428393, 0x428360, 0x428335];
    unsafe extern "cdecl" fn skiponcehost(_: *mut ilhook::x86::Registers, _: usize, _: usize) -> usize {
        if ESC > 120 { 0 } else { 1 }
    }
    std::mem::forget(ilhook::x86::Hooker::new(0x428330, HookType::JmpToEnumRet(skiponcehost_ret, skiponcehost), 0).hook(5));

    unsafe extern "cdecl" fn esc_host(_: *mut ilhook::x86::Registers, _: usize) { send_packet_untagged(Box::new([0x6e, 0])) }
    std::mem::forget(ilhook::x86::Hooker::new(0x428394, HookType::JmpBack(esc_host), 0).hook(5));

    let skiponceclient_ret = vec![0x4286c3, 0x428630, 0x428605];
    unsafe extern "cdecl" fn skiponceclient(_: *mut ilhook::x86::Registers, _: usize, _: usize) -> usize {
        if ESC > 120 { 0 } else { 1 }
    }
    std::mem::forget(ilhook::x86::Hooker::new(0x428600, HookType::JmpToEnumRet(skiponceclient_ret, skiponceclient), 0).hook(5));

    unsafe extern "cdecl" fn esc_client(_: *mut ilhook::x86::Registers, _: usize) { send_packet_untagged(Box::new([0x6e, 0])) }
    std::mem::forget(ilhook::x86::Hooker::new(0x428664, HookType::JmpBack(esc_client), 0).hook(5));
    std::mem::forget(ilhook::x86::Hooker::new(0x428681, HookType::JmpBack(esc_client), 0).hook(5));

    unsafe extern "cdecl" fn override_current_game_state(a: *mut ilhook::x86::Registers, _: usize) {
        if ESC2.load(Relaxed) != 0 {
            ESC2.store(0, Relaxed);
            if !GIRLS_ARE_TALKING {
                (*a).eax = if is_p1() { 8 } else { 9 };
            }
        }
    }
    std::mem::forget(ilhook::x86::Hooker::new(0x407f48, HookType::JmpBack(override_current_game_state), 0).hook(6));
}

// ===== Main Hooks =====

unsafe extern "cdecl" fn on_exit(_: *mut ilhook::x86::Registers, _: usize) {
    println!("on exit");
    reset_match_state();
    REQUESTED_THREAD_ID.store(0, Relaxed);
    *(0x8971C0 as *mut usize) = 0;

    if let Some(x) = NETCODER.take() {
        let r = x.receiver;
        while r.try_recv().is_ok() {}
        DATA_RECEIVER = Some(r);
    }

    if let Some(x) = ROLLBACKER.take() {
        for mut a in x.guessed {
            if !a.prev_state.has_called_never_happened && !a.prev_state.has_happened {
                a.prev_state.did_happen();
            }
        }
    }

    for a in MEMORY_RECEIVER_FREE.as_ref().unwrap().try_iter() {
        soku_heap_free!(a);
    }

    clean_replay_statics();
    DUMP_FRAME_TIME = None;
    println!("Memory leak: {} bytes", MEMORY_LEAK);
    MEMORY_LEAK = 0;
    LAST_M_LEN = 0;
    reset_camera_state();
}

unsafe extern "cdecl" fn ongirlstalk(_: *mut ilhook::x86::Registers, _: usize) {
    GIRLSTALKED = true;
    BATTLE_STARTED = false;
}

unsafe extern "cdecl" fn drawnumbers(_: *mut ilhook::x86::Registers, _: usize) {
    ui::draw_network_stats(
        NEXT_DRAW_PING, NEXT_DRAW_ROLLBACK, NEXT_DRAW_ENEMY_DELAY,
        NETCODER.as_ref().map(|n| n.max_rollback as i32),
        WARNING_FRAME_MISSING_1_COUNTDOWN, WARNING_FRAME_MISSING_2_COUNTDOWN,
        WARNING_FRAME_LOST_COUNTDOWN.load(Relaxed), WARNING_WHEN_LAGGING, *SOKU_FRAMECOUNT,
    );
    render_replay_progress_bar_and_numbers();
}

unsafe extern "cdecl" fn render_number_on_select(a: *mut ilhook::x86::Registers, _: usize) {
    let gametype_main = *(0x898688 as *const usize);
    let is_netplay = *(0x8986a0 as *const usize) != 0;
    let in_stage_select = *(((*a).esi + 0x4f60) as *const i32) >= 1;
    if (gametype_main, is_netplay, in_stage_select, TOGGLE_STAT) == (1, true, false, true) {
        draw_num((300.0, 466.0 - 16.0), MAX_ROLLBACK_PREFERENCE as i32);
        if let Some(time_data) = SELECT_SCENE_INPUT_SEND_TIME_DATA.lock().unwrap().as_ref() {
            if let Some(max_latency_to_show) = time_data.max_latency_to_be_shown {
                draw_num((300.0, 466.0), max_latency_to_show.as_millis() as i32);
            }
        }
    }
}

unsafe extern "cdecl" fn handle_raw_input(a: *mut ilhook::x86::Registers, _: usize, _: usize) {
    (*a).ebp = *ptr_wrap!(((*a).esi + 0x76c) as *const u32);
    let input_manager = (*a).ecx as usize;

    let real_input = match std::mem::replace(&mut REAL_INPUT, REAL_INPUT2.take()) {
        Some(x) => x,
        None => {
            IS_FIRST_READ_INPUTS = false;
            let f = std::mem::transmute::<usize, extern "fastcall" fn(usize)>(0x040a370);
            (f)(input_manager);
            return;
        }
    };

    if IS_FIRST_READ_INPUTS {
        let gametype_main = *(0x898688 as *const usize);
        let is_netplay = *(0x8986a0 as *const usize) != 0;
        if (gametype_main, is_netplay) == (2, false) {
            let set_key = std::mem::transmute::<usize, unsafe extern "cdecl" fn(u8, u8)>(0x0043de50);
            for k in 0x3b..=0x42 { set_key(k, 0); }
        }
    }
    IS_FIRST_READ_INPUTS = false;

    let td = &mut *ptr_wrap!((input_manager + 0x38) as *mut i32);
    let lr = &mut *ptr_wrap!((input_manager + 0x3c) as *mut i32);

    *lr = match (real_input[0], real_input[1]) {
        (false, true) => (*lr).max(0) + 1,
        (true, false) | (true, true) => (*lr).min(0) - 1,
        _ => 0,
    };
    *td = match (real_input[2], real_input[3]) {
        (false, true) => (*td).max(0) + 1,
        (true, false) | (true, true) => (*td).min(0) - 1,
        _ => 0,
    };

    for a in 0..(INPUT_KEYS_NUMBERS - 4) {
        let v = &mut *ptr_wrap!((input_manager + 0x40 + a * 4) as *mut u32);
        *v = if real_input[a + 4] { *v + 1 } else { 0 };
    }

    let m = &mut *ptr_wrap!((input_manager + 0x62) as *mut u16);
    *m = input_to_accum(&real_input);
}

unsafe extern "cdecl" fn sniff_sent(a: *mut ilhook::x86::Registers, _: usize) {
    let ptr = ((*a).edi + 0x1c) as *const u8;
    let packet_size = *(((*a).edi + 0x18) as *const usize);
    let buf = std::slice::from_raw_parts(ptr, 400);

    update_input_time_data(buf, packet_size, true);

    if !FREEZE_MITIGATION { return; }

    if buf[0] == if is_p1() { 13 } else { 14 } && buf[1] == 4 {
        if LAST_GAME_REQUEST.is_some() {
            println!("sending more game request!");
        } else {
            let mut m = [0; 400];
            m.copy_from_slice(buf);
            println!("get game request!");
            LAST_GAME_REQUEST = Some(m);
        }
    }

    if (buf[0] == 13 || buf[0] == 14) && buf[1] == 2 {
        let mut m = [0; 400];
        m.copy_from_slice(buf);
        LAST_LOAD_ACK = Some(m);
    }

    if (buf[0] == 13 || buf[0] == 14) && buf[1] == 5 {
        let mut m = [0; 400];
        m.copy_from_slice(buf);
        LAST_MATCH_ACK = Some(m);
    }

    if (buf[0] == 13 || buf[0] == 14) && buf[1] == 1 {
        let mut m = [0; 400];
        m.copy_from_slice(buf);
        LAST_MATCH_LOAD = Some(m);
    }
}

unsafe extern "stdcall" fn recvfrom_with_fake_packet(s: SOCKET, buf: *mut u8, len: i32, flags: i32, from: *mut SOCKADDR, fromlen: *mut i32) -> u32 {
    if let Some(ori_recvfrom) = ORI_RECVFROM {
        if AFTER_GAME_REQUEST_FROM_P1 {
            let netmanager = *(0x8986a0 as *const usize);
            let to = if *(netmanager as *const usize) == 0x858cac {
                *((netmanager + 0x4c8) as *const *const SOCKADDR)
            } else {
                (netmanager + 0x47c) as *const SOCKADDR
            };
            *from = *to;
            *fromlen = 0x10;
            *buf = 0xd;
            *buf.offset(1) = 0x5;
            println!("Send a simulated LAST_MATCH_ACK packet to myself to get my GAME_REQUEST.");
            return 2;
        }
        return ori_recvfrom(s, buf, len, flags, from, fromlen);
    }
    panic!();
}

fn update_input_time_data(buf: &[u8], packet_size: usize, is_sending: bool) {
    if packet_size >= 10 && matches!(buf[0], 0xe | 0xd) && buf[1] == 0x3 && buf[6] == 0x3 {
        let mut guard = SELECT_SCENE_INPUT_SEND_TIME_DATA.lock().unwrap();
        if let Some(time_data) = guard.as_mut() {
            let input_count: u8 = buf[7];
            let input_pair_count = input_count.div_ceil(2);
            let frame_id_end = u32::from_le_bytes(buf[2..6].try_into().unwrap()) as usize;
            let frame_id = frame_id_end + 1 - input_pair_count as usize;

            if !is_sending && time_data.last_frame_id <= frame_id && !time_data.has_received {
                time_data.has_received = true;
                time_data.last_frame_id = frame_id_end;
                time_data.last_max_latency = Some(
                    (Instant::now().saturating_duration_since(time_data.last_receive_time) / 2)
                        .max(time_data.last_max_latency.unwrap_or(Duration::ZERO)),
                );
                if frame_id > time_data.last_shown_frame + 60 {
                    time_data.max_latency_to_be_shown = time_data.last_max_latency;
                    time_data.last_shown_frame = frame_id;
                    time_data.last_max_latency = None;
                }
            }

            if is_sending && time_data.has_received {
                let to_get_frame_id = match buf[0] { 0xd => frame_id_end, 0xe => frame_id + 1, _ => panic!() };
                if time_data.last_frame_id < to_get_frame_id {
                    time_data.has_received = false;
                    time_data.last_receive_time = Instant::now();
                    time_data.last_frame_id = to_get_frame_id;
                }
            }
        }
    }
}

unsafe extern "cdecl" fn readonlinedata(a: *mut ilhook::x86::Registers, _: usize) {
    const P1_PACKETS: [u8; 400] = [13, 3, 1, 0, 0, 0, 5, 2, 0, 0, 0, 0, 12, 0, 103, 0, 103, 0, 103, 0, 103, 0, 104, 0, 104, 0, 104, 0, 104, 0, 106, 0, 106, 0, 200, 0, 203, 0, 208, 0, 208, 0, 208, 0, 208, 0, 1, 15, 0, 0, 0, 189, 3, 21, 23, 251, 48, 70, 108, 0, 0, 0, 0, 0, 0, 221, 143, 113, 190, 134, 199, 125, 39, 12, 12, 64, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
    const P2_PACKETS: [u8; 400] = [14, 3, 0, 0, 0, 0, 5, 1, 0, 0, 20, 100, 0, 100, 0, 101, 0, 101, 0, 102, 0, 102, 0, 103, 0, 103, 0, 200, 0, 200, 0, 200, 0, 200, 0, 201, 0, 201, 0, 201, 0, 201, 0, 203, 0, 203, 0, 203, 0, 203, 0, 1, 15, 119, 144, 191, 37, 118, 8, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];

    let esp = (*a).esp;
    let slic = std::slice::from_raw_parts_mut((esp + 0x70) as *mut u8, 400);
    let len: i32 = (*a).eax as i32;
    let type1 = if len > 0 { slic[0] } else { 0 };
    let type2 = slic[1];

    if len < 0 { println!("WARNING: recvfrom returned error."); }

    update_input_time_data(slic, len as usize, false);

    if type1 == 0x6e && BATTLE_STARTED {
        ESC2.store(1, Relaxed);
        slic.copy_from_slice(if !is_p1() { &P1_PACKETS } else { &P2_PACKETS });
    } else if type1 == 0x6c {
        let buf = [0x6d, 0x61];
        let sock = *ptr_wrap!(((*a).edi + 0x28) as *const u32);
        let to = (*a).esp + 0x44;
        windows::Win32::Networking::WinSock::sendto(std::mem::transmute::<u32, SOCKET>(sock), &buf, 0, to as _, 0x10);
        (*a).eax = 0x400;
    } else if type1 > 0x6c && type1 <= 0x80 {
        (*a).eax = 0x400;
    }

    if type1 == 0x6b {
        let m = DISABLE_SEND.load(Relaxed);
        if BATTLE_STARTED {
            DATA_SENDER.as_ref().unwrap().send((NetworkPacket::decode(&slic[..len as usize]), Instant::now())).unwrap();
        }
        if m < 150 {
            DISABLE_SEND.store(m + 1, Relaxed);
            slic.copy_from_slice(if !is_p1() { &P1_PACKETS } else { &P2_PACKETS });
        } else {
            (*a).eax = 0x400;
        }
    }

    if type1 == 14 || type1 == 13 {
        if FREEZE_MITIGATION {
            if type2 == 4 {
                if HAS_LOADED {
                    println!("Receive redundance GAME_REQUEST. Ignore it.");
                    slic[0] = 0;
                } else if type1 == 13 {
                    println!("Receive GAME_REQUEST.");
                    if matches!(*(0x008A0044 as *const u32), 8 | 9) && !is_p1() {
                        AFTER_GAME_REQUEST_FROM_P1 = true;
                        println!("It is from p1 to p2. A fake packet will be sent.");
                    }
                }
                HAS_LOADED = true;
            }

            if type2 == 5 && [8usize, 9, 10, 11].contains(&*(0x008A0044 as *const usize)) && !is_p1() {
                if AFTER_GAME_REQUEST_FROM_P1 {
                    println!("p2 get its fake packet [{},{}]", type1, type2);
                    AFTER_GAME_REQUEST_FROM_P1 = false;
                } else {
                    println!("p2 get [{},{}]. not send it to the game.", type1, type2);
                    slic[0] = 0;
                    if let Some(gr) = LAST_GAME_REQUEST {
                        println!("the opponent is requesting GAME_REQUEST packet. reply it.");
                        send_packet_untagged(Box::new(gr));
                    }
                }
            }
        }

        if type2 == 1 && BATTLE_STARTED {
            ESC += 1;
            if ESC == 10 {
                slic.copy_from_slice(if !is_p1() { &P1_PACKETS } else { &P2_PACKETS });
            }
            if ESC > 250 {
                println!("here stuck state detected");
                slic[0] = 0xb;
                ESC = 0;
                send_packet_untagged(Box::new([0xb]));
                let netmanager = *(0x8986a0 as *const usize);
                closesocket(*((netmanager + 0x3e4) as *const SOCKET));
            }
        }
    }

    if type1 == 5 && slic[25] == 0 {
        slic[1] = if F62_ENABLED { VERSION_BYTE_62 } else { VERSION_BYTE_60 };
    }
}

// ===== Online/Replay Handlers =====

unsafe fn handle_online(framecount: usize, battle_state: &mut u32, cur_speed: &mut u32, cur_speed_iter: &mut u32, state_sub_count: &mut u32) {
    if framecount == 0 && !BATTLE_STARTED {
        let round = *ptr_wrap!((*(0x8986a0 as *const usize) + 0x6c0) as *const u8);
        BATTLE_STARTED = true;
        SOUND_MANAGER = Some(RollbackSoundManager::new());
        let m = DATA_RECEIVER.take().unwrap();
        ROLLBACKER = Some(Rollbacker::new());
        let mut netcoder = Netcoder::new(m, MAX_ROLLBACK_PREFERENCE);
        if round == 1 {
            netcoder.autodelay_enabled = if AUTODELAY_ENABLED { Some(AUTODELAY_ROLLBACK) } else { None };
            netcoder.delay = DEFAULT_DELAY_VALUE;
        } else {
            netcoder.delay = LAST_DELAY_VALUE;
        }
        netcoder.max_rollback = 6;
        netcoder.display_stats = TOGGLE_STAT;
        NETCODER = Some(netcoder);
        if smoothing_allowed() { enable_runtime_smoothing(); }
    }

    if *battle_state == 6 { GIRLS_ARE_TALKING = true; }

    let rollbacker = ROLLBACKER.as_mut().unwrap();
    let netcoder = NETCODER.as_mut().unwrap();

    resume(battle_state);
    update_toggle_stat_from_keys();
    netcoder.display_stats = TOGGLE_STAT;

    if *cur_speed_iter == 0 {
        LAST_DELAY_VALUE = change_delay_from_keys(netcoder.delay);
        netcoder.delay = LAST_DELAY_VALUE;
        let input = read_current_input();
        let speed = netcoder.process_and_send(rollbacker, input);
        *cur_speed = speed;

        #[cfg(feature = "lowframetest")]
        {
            use rand::Rng;
            static mut RNG: Option<rand::rngs::ThreadRng> = None;
            if RNG.is_none() { RNG = Some(rand::thread_rng()); }
            let target_frametime = timing::target_frametime();
            std::thread::sleep(Duration::from_micros(RNG.as_mut().unwrap().gen_range(target_frametime * 3 / 4..=target_frametime) as u64));
        }

        if speed == 0 { pause(battle_state, state_sub_count); return; }
    }

    if rollbacker.step(*cur_speed_iter as usize).is_none() {
        pause(battle_state, state_sub_count);
    }
}

unsafe extern "cdecl" fn main_hook(a: *mut ilhook::x86::Registers, _: usize) {
    let framecount = *SOKU_FRAMECOUNT;
    let w = (*a).esi;
    let mut cur_speed = (*a).ebx;
    let mut cur_speed_iter = (*a).edi;
    let battle_state = &mut *((w + 4 * 0x22) as *mut u32);
    let state_sub_count = &mut *ptr_wrap!((w + 4) as *mut u32);

    let gametype_main = *(0x898688 as *const usize);
    let is_netplay = *(0x8986a0 as *const usize) != 0;
    IS_FIRST_READ_INPUTS = true;

    if framecount == 0 { clear_smoothed_transform(); }

    match (gametype_main, is_netplay) {
        (2, false) => {
            if framecount > 0 { REQUESTED_THREAD_ID.store(GetCurrentThreadId(), Relaxed); }
            handle_replay(framecount, battle_state, &mut cur_speed, &mut cur_speed_iter, state_sub_count, &TAKEOVER_KEYS_SCHEME);
        }
        (1, true) => {
            if framecount > 0 {
                REQUESTED_THREAD_ID.store(GetCurrentThreadId(), Relaxed);
            } else if let Some(fake) = FAKE_BATTLE_MANAGER_FOR_TSK.as_mut() {
                fake.fake_left_win_count = 0;
                fake.fake_right_win_count = 0;
                fake.fake_battle_mode = *battle_state;
            }
            if !GIRLSTALKED { handle_online(framecount, battle_state, &mut cur_speed, &mut cur_speed_iter, state_sub_count); }
        }
        _ => (),
    }

    let is_story_or_result_mode = matches!(*(0x00898690 as *const u32), 0 | 7);
    if !is_story_or_result_mode && matches!(*battle_state, 3 | 5) && *state_sub_count == 1 {
        std::mem::transmute::<usize, extern "stdcall" fn(u32)>(0x439490)(0x2c);
    }

    if cur_speed_iter + 1 >= cur_speed {
        WARNING_FRAME_MISSING_1_COUNTDOWN = WARNING_FRAME_MISSING_1_COUNTDOWN.saturating_sub(1);
        WARNING_FRAME_MISSING_2_COUNTDOWN = WARNING_FRAME_MISSING_2_COUNTDOWN.saturating_sub(1);
    }

    let battle_manager = (*a).esi as *const *const u8;
    if *battle_state == 5 && NETCODER.is_some() && *state_sub_count as usize > 15 {
        if let Some(fake) = FAKE_BATTLE_MANAGER_FOR_TSK.as_mut() {
            fake.fake_left_win_count = *(*battle_manager.offset(3)).offset(0x573);
            fake.fake_right_win_count = *(*battle_manager.offset(4)).offset(0x573);
            fake.fake_battle_mode = 5;
        }
    }

    (*a).ebx = cur_speed;
    (*a).edi = cur_speed_iter;
}

#[cfg(test)]
mod input_to_accum_tests;
