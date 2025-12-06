use std::ops::{Add, Div, Mul, Sub};

use winapi::shared::{
    d3d9::IDirect3DDevice9,
    d3d9types::{D3DCLEAR_TARGET, D3DCOLOR, D3DCOLOR_ARGB, D3DRECT},
};

use crate::{println, GiurollConfig, SOKU_FRAMECOUNT};

#[derive(Debug, Clone, Copy)]
#[repr(C)]
struct F32 {
    f: f32,
}

impl PartialEq for F32 {
    fn eq(&self, other: &Self) -> bool {
        self.f.to_ne_bytes() == other.f.to_ne_bytes()
    }
}
impl Eq for F32 {}

#[derive(Debug, Clone, Copy)]
#[repr(C)]
struct XY {
    x: f32,
    y: f32,
}

impl PartialEq for XY {
    fn eq(&self, other: &Self) -> bool {
        self.x.to_ne_bytes() == other.x.to_ne_bytes()
            && self.y.to_ne_bytes() == other.y.to_ne_bytes()
    }
}
impl Eq for XY {}
impl Add<XY> for XY {
    type Output = XY;

    fn add(self, rhs: XY) -> Self::Output {
        Self {
            x: self.x + rhs.x,
            y: self.y + rhs.y,
        }
    }
}

impl Sub<XY> for XY {
    type Output = XY;

    fn sub(self, rhs: XY) -> Self::Output {
        Self {
            x: self.x - rhs.x,
            y: self.y - rhs.y,
        }
    }
}

impl Mul<f32> for XY {
    type Output = XY;

    fn mul(self, rhs: f32) -> Self::Output {
        Self {
            x: self.x * rhs,
            y: self.y * rhs,
        }
    }
}

impl Div<f32> for XY {
    type Output = XY;

    fn div(self, rhs: f32) -> Self::Output {
        Self {
            x: self.x / rhs,
            y: self.y / rhs,
        }
    }
}

impl XY {
    fn dot_prod(self, rhs: XY) -> f32 {
        self.x * rhs.x + self.y * rhs.y
    }

    fn projection_of(self, rhs: XY) -> XY {
        self * (self.dot_prod(rhs) / self.dot_prod(self))
    }
}

#[derive(Debug, PartialEq, Eq, Clone)]
struct CameraTransform {
    scale_affected_only_by_smooth: F32,
    xy_affected_only_by_smooth: XY,
    shake_degress_affected_only_by_smooth: [F32; 2],
    shake_affected_by_game_and_smooth: F32,
    determined_by_smooth1: [F32; 2],
    determined_by_smooth2: [F32; 4],
    determined_by_smooth3: [F32; 2],
}
impl CameraTransform {
    unsafe fn dump() -> Self {
        let camera: usize = 0x00898600;
        Self {
            scale_affected_only_by_smooth: *((camera + 0x14) as *const F32),
            xy_affected_only_by_smooth: *((camera + 0x18) as *const XY),
            shake_degress_affected_only_by_smooth: *((camera + 0x38) as *const [F32; 2]),
            shake_affected_by_game_and_smooth: *((camera + 0x40) as *const F32),
            determined_by_smooth3: *((camera + 0x30) as *const [F32; 2]),
            determined_by_smooth1: *((camera + 0x0c) as *const [F32; 2]),
            determined_by_smooth2: *((camera + 0x5c) as *const [F32; 4]),
        }
    }
    unsafe fn restore_all(&self) -> Self {
        let ori = Self::dump();
        self.restore_affected_only_by_smooth();
        self.restore_shake_affected_by_game_and_smooth();
        self.restore_determined_by_smooth();
        ori
    }
    unsafe fn restore_affected_only_by_smooth(&self) {
        let camera: usize = 0x00898600;
        *((camera + 0x14) as *mut F32) = self.scale_affected_only_by_smooth;
        *((camera + 0x18) as *mut XY) = self.xy_affected_only_by_smooth;
        *((camera + 0x38) as *mut [F32; 2]) = self.shake_degress_affected_only_by_smooth;
    }
    unsafe fn restore_shake_affected_by_game_and_smooth(&self) {
        let camera: usize = 0x00898600;
        *((camera + 0x40) as *mut F32) = self.shake_affected_by_game_and_smooth;
    }
    unsafe fn restore_determined_by_smooth(&self) {
        let camera: usize = 0x00898600;
        *((camera + 0x0c) as *mut [F32; 2]) = self.determined_by_smooth1;
        *((camera + 0x5c) as *mut [F32; 4]) = self.determined_by_smooth2;
        *((camera + 0x30) as *mut [F32; 2]) = self.determined_by_smooth3;
    }
    unsafe fn validate_after_partially_modified(&mut self) {
        if self.shake_affected_by_game_and_smooth.f <= 1.0 {
            self.determined_by_smooth3 = [F32 { f: 0.0 }, F32 { f: 0.0 }];
        }
    }
    unsafe fn get_target_xy() -> XY {
        let camera: usize = 0x00898600;
        *((camera + 0) as *const XY)
    }
    unsafe fn get_target_scale() -> f32 {
        let camera: usize = 0x00898600;
        *((camera + 0x8) as *const f32)
    }
}

static mut CAMERA_ACTUAL_SMOOTH_TRANSFORM: Option<CameraTransform> = None;
static mut LAST_IDEAL_CAMERA: Option<CameraTransform> = None;
static mut LAST_CAMERA_BEFORE_SMOOTH: Option<CameraTransform> = None;
static mut SMOOTH_ENABLED_CONFIG: bool = true;
static mut SMOOTH_INCREASING_SCALE_CORRECTION: Option<f32> = None;
static mut SMOOTH_DECREASING_SCALE_CORRECTION: Option<f32> = None;
static mut SMOOTH_X_CORRECTION: Option<f32> = None;
static mut SMOOTH_Y_CORRECTION: Option<f32> = None;
static mut SMOOTH: bool = false;

static mut LAST_SMOOTHED_FRAMECOUNT: usize = 0;

pub(crate) fn apply_config(config: &GiurollConfig) {
    unsafe {
        SMOOTH_ENABLED_CONFIG = config.smooth_camera;
        let half_life_to_correction = |half_life: i64| match half_life {
            0 => 1.0,
            x => 1.0 - 0.5_f32.powf(1.0 / x.max(1) as f32),
        };
        SMOOTH_INCREASING_SCALE_CORRECTION = Some(half_life_to_correction(
            config.smooth_decreasing_scale_correction,
        ));
        SMOOTH_DECREASING_SCALE_CORRECTION = Some(half_life_to_correction(
            config.smooth_increasing_scale_correction,
        ));
        SMOOTH_X_CORRECTION = Some(half_life_to_correction(config.smooth_x_correction));
        SMOOTH_Y_CORRECTION = Some(half_life_to_correction(config.smooth_y_correction));
    }
}

pub(crate) fn reset_state() {
    unsafe {
        CAMERA_ACTUAL_SMOOTH_TRANSFORM = None;
        LAST_IDEAL_CAMERA = None;
        LAST_CAMERA_BEFORE_SMOOTH = None;
        SMOOTH = false;
    }
}

pub(crate) fn clear_smoothed_transform() {
    unsafe {
        CAMERA_ACTUAL_SMOOTH_TRANSFORM = None;
    }
}

pub(crate) fn enable_runtime_smoothing() {
    unsafe {
        SMOOTH = true;
    }
}

pub(crate) fn smoothing_allowed() -> bool {
    unsafe { SMOOTH_ENABLED_CONFIG }
}

pub(crate) unsafe extern "thiscall" fn save_last_transform(camera: usize) {
    LAST_CAMERA_BEFORE_SMOOTH = Some(CameraTransform::dump());
    let transform_smoothly: unsafe extern "thiscall" fn(usize) = std::mem::transmute(0x429040);
    transform_smoothly(camera);
}

pub(crate) unsafe fn cbattle_process_smooth(
    cbattle_process: unsafe extern "thiscall" fn(usize) -> usize,
    cbattle: usize,
) -> usize {
    if let Some(last_ideal) = LAST_IDEAL_CAMERA.take() {
        last_ideal.restore_all();
    }
    let ret = cbattle_process(cbattle);
    if SMOOTH {
        let ideal = CameraTransform::dump();
        if let Some(mut last_smoothed) = CAMERA_ACTUAL_SMOOTH_TRANSFORM.take() {
            if LAST_SMOOTHED_FRAMECOUNT > *SOKU_FRAMECOUNT {
                println!(
                    "Smooth when rewinding? last: {}, current: {}",
                    LAST_SMOOTHED_FRAMECOUNT, *SOKU_FRAMECOUNT
                );
            } else if LAST_SMOOTHED_FRAMECOUNT == *SOKU_FRAMECOUNT {
                last_smoothed.restore_all();
            } else {
                if LAST_SMOOTHED_FRAMECOUNT + 1 < *SOKU_FRAMECOUNT {
                    println!(
                        "Smooth when fast forwarding? last: {}, current: {}",
                        LAST_SMOOTHED_FRAMECOUNT, *SOKU_FRAMECOUNT
                    );
                }
                if let Some(before) = LAST_CAMERA_BEFORE_SMOOTH.as_ref() {
                    last_smoothed.shake_affected_by_game_and_smooth = F32 {
                        f: before.shake_affected_by_game_and_smooth.f,
                    };

                    let clamp_unordered =
                        |a: f32, o1: f32, o2: f32| a.clamp(o1.min(o2), o1.max(o2));
                    let target_scale = CameraTransform::get_target_scale();
                    let scale = last_smoothed.scale_affected_only_by_smooth.f;
                    let actual_scale = before.scale_affected_only_by_smooth.f;
                    let diff_to_target = target_scale - scale;
                    let diff_to_actual = actual_scale - scale;
                    let diff = clamp_unordered(diff_to_actual, 0.0, diff_to_target);
                    if diff.abs() >= 0.01 {
                        if target_scale > scale {
                            if let Some(c) = SMOOTH_INCREASING_SCALE_CORRECTION {
                                last_smoothed.scale_affected_only_by_smooth.f += diff * c;
                            }
                        } else if let Some(c) = SMOOTH_DECREASING_SCALE_CORRECTION {
                            last_smoothed.scale_affected_only_by_smooth.f += diff * c;
                        }
                    }

                    let target_xy = CameraTransform::get_target_xy();
                    let xy = last_smoothed.xy_affected_only_by_smooth;
                    let actual_xy = before.xy_affected_only_by_smooth;
                    let diff_to_target = target_xy - xy;
                    let diff_to_actual = actual_xy - xy;
                    if diff_to_target.x.abs() > 1.0 || diff_to_target.y.abs() > 1.0 {
                        let mut diff = diff_to_target.projection_of(diff_to_actual);
                        diff.x = clamp_unordered(diff.x, 0.0, diff_to_target.x);
                        diff.y = clamp_unordered(diff.y, 0.0, diff_to_target.y);
                        if diff.x.abs() > 1.0 || diff.y.abs() > 1.0 {
                            if let Some(c) = SMOOTH_X_CORRECTION {
                                last_smoothed.xy_affected_only_by_smooth.x = xy.x + diff.x * c;
                            }
                            if let Some(c) = SMOOTH_Y_CORRECTION {
                                last_smoothed.xy_affected_only_by_smooth.y = xy.y + diff.y * c;
                            }
                        }
                    }

                    last_smoothed.validate_after_partially_modified();
                }
                last_smoothed.restore_all();
                let transform_smoothly: unsafe extern "thiscall" fn(usize) =
                    std::mem::transmute(0x429040);
                let camera: usize = 0x00898600;
                transform_smoothly(camera);
            }
            let smoothed = CameraTransform::dump();
            assert_eq!(
                smoothed.shake_affected_by_game_and_smooth,
                ideal.shake_affected_by_game_and_smooth
            );
        }
        CAMERA_ACTUAL_SMOOTH_TRANSFORM = Some(CameraTransform::dump());
        LAST_SMOOTHED_FRAMECOUNT = *SOKU_FRAMECOUNT;
        assert!(LAST_IDEAL_CAMERA.is_none());
        LAST_IDEAL_CAMERA = Some(ideal);
    }
    ret
}

pub(crate) unsafe fn draw_block(device: *const IDirect3DDevice9, inner: &D3DRECT, color: D3DCOLOR) {
    let border = D3DRECT {
        x1: inner.x1.min(inner.x2) - 2,
        x2: inner.x1.max(inner.x2) + 2,
        y1: inner.y1.min(inner.y2) - 2,
        y2: inner.y1.max(inner.y2) + 2,
    };
    (*device).Clear(
        1,
        &border,
        D3DCLEAR_TARGET,
        D3DCOLOR_ARGB(0xff, 0, 0, 0),
        0.0,
        0,
    );
    (*device).Clear(1, inner, D3DCLEAR_TARGET, color, 0.0, 0);
}
