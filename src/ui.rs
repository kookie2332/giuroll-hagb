//! UI rendering functions for displaying network stats and game information.

use std::ffi::c_void;

use winapi::shared::d3d9::IDirect3DDevice9;
use winapi::shared::d3d9types::{D3DCOLOR_ARGB, D3DRECT};

use crate::camera::draw_block;

/// Draw a number at the given position using Soku's built-in font renderer.
pub fn draw_num(pos: (f32, f32), num: i32) {
    let drawfn: extern "thiscall" fn(
        ptr: *const c_void,
        number: i32,
        x: f32,
        y: f32,
        a1: i32,
        a2: u8,
    ) = unsafe { std::mem::transmute::<usize, _>(0x414940) };

    drawfn(0x882940 as *const c_void, num, pos.0, pos.1, 0, 0);
}

/// Calculate the pixel width of a number when rendered.
pub fn get_num_length(num: i32, edge_spacing: bool) -> f32 {
    let mut len: usize = 0;
    let mut num_ = num;
    while num_ != 0 {
        num_ /= 10;
        len += 1;
    }
    if len == 0 {
        len = 1;
    }
    let width = unsafe { *((0x882940 + 0x4) as *const f32) };
    let spacing = unsafe { *((0x882940 + 0x8) as *const f32) };
    let scale = unsafe { *((0x882940 + 0xc) as *const f32) };
    (width * (len as f32)
        + spacing * (if edge_spacing { len + 1 } else { len - 1 } as f32))
        * scale
}

/// Draw a number centered horizontally at the given position.
pub fn draw_num_x_center(pos: (f32, f32), num: i32) {
    let drawfn: extern "thiscall" fn(
        ptr: *const c_void,
        number: i32,
        x: f32,
        y: f32,
        a1: i32,
        a2: u8,
    ) = unsafe { std::mem::transmute::<usize, _>(0x414940) };
    drawfn(
        0x882940 as *const c_void,
        num,
        pos.0 + get_num_length(num, false) / 2.0,
        pos.1,
        0,
        0,
    );
}

/// Draw the network statistics overlay (ping, rollback, delay, FPS).
pub unsafe fn draw_network_stats(
    next_draw_ping: Option<i32>,
    next_draw_rollback: Option<i32>,
    next_draw_enemy_delay: Option<i32>,
    max_rollback: Option<i32>,
    warning_frame_missing_1: usize,
    warning_frame_missing_2: usize,
    warning_frame_lost: u32,
    warning_when_lagging: bool,
    framecount: usize,
) {
    let d3d9_device = 0x008A0E30 as *const *const IDirect3DDevice9;
    let yellow = D3DCOLOR_ARGB(0xff, 0xff, 0xff, 0);
    let red = D3DCOLOR_ARGB(0xff, 0xff, 0, 0);

    if let Some(x) = next_draw_ping {
        if warning_frame_missing_1 != 0 && warning_when_lagging && framecount >= 120 {
            let inner = D3DRECT {
                x1: 300 - get_num_length(next_draw_ping.unwrap_or(10), false) as i32,
                x2: 300 + 2,
                y1: 466,
                y2: 480 - 2,
            };
            draw_block(*d3d9_device, &inner, yellow);
        }
        draw_num((300.0, 466.0), x);
    }

    if let Some(x) = next_draw_rollback {
        if warning_frame_missing_2 != 0 && warning_when_lagging && framecount >= 120 {
            let inner = D3DRECT {
                x1: 325 - get_num_length(next_draw_rollback.unwrap_or(1), false) as i32,
                x2: 325 + 2,
                y1: 466,
                y2: 480 - 2,
            };
            draw_block(*d3d9_device, &inner, yellow);
        }
        draw_num((325.0, 466.0), x);
        if let Some(mr) = max_rollback {
            draw_num((350.0, 466.0), mr);
        }
    }

    if let Some(x) = next_draw_enemy_delay {
        draw_num((20.0, 466.0), x);
    }

    if warning_frame_lost != 0
        && warning_when_lagging
        && *(0x8998b2 as *const bool) /* whether display fps */
        && framecount >= 120
    {
        let inner = D3DRECT {
            x1: 640 - get_num_length(60, false) as i32,
            x2: 640 + 2,
            y1: 466,
            y2: 480 - 2,
        };
        draw_block(*d3d9_device, &inner, red);
    }
}
