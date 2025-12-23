use std::{collections::HashMap, path::PathBuf};

use mininip::datas::{Identifier, Value};
use winapi::shared::d3d9types::{D3DCOLOR, D3DCOLOR_ARGB};

use crate::{ISDEBUG, VERSION_STR};

pub struct GiurollConfig {
    pub increase_delay_key: i64,
    pub decrease_delay_key: i64,
    pub decrease_max_rollback_key: i64,
    pub increase_max_rollback_key: i64,
    pub toggle_network_stats: i64,
    pub exit_takeover: i64,
    pub p1_takeover: i64,
    pub p2_takeover: i64,
    pub set_or_retry_takeover: i64,
    pub spin_amount: i64,
    pub enable_f62: bool,
    pub enable_network_stats_by_default: bool,
    pub default_delay: i64,
    pub auto_delay_enabled: bool,
    pub freeze_mitigation: bool,
    pub auto_delay_rollback: i64,
    pub smooth_camera: bool,
    pub smooth_decreasing_scale_correction: i64,
    pub smooth_increasing_scale_correction: i64,
    pub smooth_x_correction: i64,
    pub smooth_y_correction: i64,
    pub max_rollback_preference: u8,
    pub warning_when_lagging: bool,
    pub soku2_compatibility_mode: bool,
    pub enable_println: bool,
    pub enable_check_mode: bool,
    pub turning_off_all_extra_ui: bool,
    pub default_delay_takeover: i64,
    pub outer_color: D3DCOLOR,
    pub inside_color: D3DCOLOR,
    pub progress_color: D3DCOLOR,
    pub takeover_color: D3DCOLOR,
    pub center_x_p1: i64,
    pub center_y_p1: i64,
    pub center_x_p2: i64,
    pub center_y_p2: i64,
    pub inside_half_height: i64,
    pub inside_half_width: i64,
    pub outer_half_height: i64,
    pub outer_half_width: i64,
    pub title: String,
}

impl GiurollConfig {
    pub fn from_file(filepath: PathBuf, pretend_to_be_vanilla: bool) -> Result<Self, String> {
        let conf = mininip::parse::parse_file(filepath)
            .map_err(|e| format!("Failed to parse ini: {}", e))?;

        let inc = read_ini_int_hex(&conf, "Keyboard", "increase_delay_key", 0);
        let dec = read_ini_int_hex(&conf, "Keyboard", "decrease_delay_key", 0);
        let rdec = read_ini_int_hex(&conf, "Keyboard", "decrease_max_rollback_key", 0x0a);
        let rinc = read_ini_int_hex(&conf, "Keyboard", "increase_max_rollback_key", 0x0b);
        let net = read_ini_int_hex(&conf, "Keyboard", "toggle_network_stats", 0);
        let exit_takeover = read_ini_int_hex(&conf, "Keyboard", "exit_takeover", 0x10);
        let p1_takeover = read_ini_int_hex(&conf, "Keyboard", "p1_takeover", 0x21);
        let p2_takeover = read_ini_int_hex(&conf, "Keyboard", "p2_takeover", 0x22);
        let set_or_retry_takeover =
            read_ini_int_hex(&conf, "Keyboard", "set_or_retry_takeover", 0x13);
        let spin = read_ini_int_hex(&conf, "FramerateFix", "spin_amount", 1500);
        let enable_f62 = read_ini_bool(&conf, "FramerateFix", "enable_f62", cfg!(feature = "f62"));
        let network_menu =
            read_ini_bool(&conf, "Netplay", "enable_network_stats_by_default", false);
        let default_delay = read_ini_int_hex(&conf, "Netplay", "default_delay", 2).clamp(0, 9);
        let auto_delay_enabled = read_ini_bool(&conf, "Netplay", "enable_auto_delay", true);
        let freeze_mitigation = read_ini_bool(&conf, "Netplay", "freeze_mitigation__", false);
        let auto_delay_rollback = read_ini_int_hex(&conf, "Netplay", "auto_delay_rollback", 0);
        let smooth_camera = read_ini_bool(&conf, "Netplay", "smooth_camera", true);
        let smooth_decreasing_scale_correction = read_ini_int_hex(
            &conf,
            "SmoothCamera",
            "decreasing_scale_correction_half_life__",
            15,
        );
        let smooth_increasing_scale_correction = read_ini_int_hex(
            &conf,
            "SmoothCamera",
            "increasing_scale_correction_half_life__",
            60,
        );
        let smooth_x_correction =
            read_ini_int_hex(&conf, "SmoothCamera", "x_correction_half_life__", 21);
        let smooth_y_correction =
            read_ini_int_hex(&conf, "SmoothCamera", "y_correction_half_life__", 21);
        let max_rollback_preference =
            read_ini_int_hex(&conf, "Netplay", "max_rollback_preference", 6).clamp(0, 15) as u8;
        let warning_when_lagging = read_ini_bool(&conf, "Misc", "warning_when_lagging", true);
        let soku2_compat_mode = read_ini_bool(&conf, "Misc", "soku2_compatibility_mode", false);
        let enable_println = read_ini_bool(
            &conf,
            "Misc",
            "enable_println",
            cfg!(feature = "allocconsole") || ISDEBUG,
        );
        let enable_check_mode = read_ini_bool(&conf, "Misc", "enable_check_mode", false);
        let turning_off_all_extra_ui = read_ini_bool(
            &conf,
            "Misc",
            "turning_off_all_extra_ui",
            pretend_to_be_vanilla,
        );
        let default_delay_takeover =
            read_ini_int_hex(&conf, "Takeover", "default_delay", 0).clamp(0, 9);
        let outer_color: D3DCOLOR = read_ini_int_hex(
            &conf,
            "Takeover",
            "progress_bar_outer_color",
            D3DCOLOR_ARGB(0xff, 0xff, 0, 0) as i64,
        ) as D3DCOLOR;
        let inside_color: D3DCOLOR = read_ini_int_hex(
            &conf,
            "Takeover",
            "progress_bar_inside_color",
            D3DCOLOR_ARGB(0xff, 0, 0, 0xff) as i64,
        ) as D3DCOLOR;
        let progress_color: D3DCOLOR = read_ini_int_hex(
            &conf,
            "Takeover",
            "progress_bar_progress_color",
            D3DCOLOR_ARGB(0xff, 0xff, 0xff, 0) as i64,
        ) as D3DCOLOR;
        let takeover_color: D3DCOLOR = read_ini_int_hex(
            &conf,
            "Takeover",
            "takeover_color",
            D3DCOLOR_ARGB(0xff, 0, 0xff, 0) as i64,
        ) as D3DCOLOR;
        let center_x_p1 = read_ini_int_hex(&conf, "Takeover", "progress_bar_center_x_p1", 224);
        let center_y_p1 = read_ini_int_hex(&conf, "Takeover", "progress_bar_center_y_p1", 428);
        let center_x_p2 =
            read_ini_int_hex(&conf, "Takeover", "progress_bar_center_x_p2", 640 - 224);
        let center_y_p2 = read_ini_int_hex(&conf, "Takeover", "progress_bar_center_y_p2", 428);
        let inside_half_height =
            read_ini_int_hex(&conf, "Takeover", "progress_bar_inside_half_height", 7);
        let inside_half_width =
            read_ini_int_hex(&conf, "Takeover", "progress_bar_inside_half_width", 58);
        let outer_half_height =
            read_ini_int_hex(&conf, "Takeover", "progress_bar_outer_half_height", 9);
        let outer_half_width =
            read_ini_int_hex(&conf, "Takeover", "progress_bar_outer_half_width", 60);

        let mut verstr: String = VERSION_STR.to_string();
        if let Some(remark) = option_env!("VERSION_REMARK") {
            verstr += " ";
            verstr += remark;
        }
        #[cfg(feature = "lowframetest")]
        {
            verstr += " low_frame_test";
        };
        if enable_f62 {
            verstr += " CN";
        }
        let title = read_ini_string(
            &conf,
            "Misc",
            "game_title",
            match pretend_to_be_vanilla {
                true => "% + $",
                false => "Touhou Hisoutensoku + $",
            }
            .to_string(),
        );

        let verstr = format!("Giuroll {}", verstr);
        let title = title.replace('$', &verstr);

        Ok(Self {
            increase_delay_key: inc,
            decrease_delay_key: dec,
            decrease_max_rollback_key: rdec,
            increase_max_rollback_key: rinc,
            toggle_network_stats: net,
            exit_takeover,
            p1_takeover,
            p2_takeover,
            set_or_retry_takeover,
            spin_amount: spin,
            enable_f62,
            enable_network_stats_by_default: network_menu,
            default_delay,
            auto_delay_enabled,
            freeze_mitigation,
            auto_delay_rollback,
            smooth_camera,
            smooth_decreasing_scale_correction,
            smooth_increasing_scale_correction,
            smooth_x_correction,
            smooth_y_correction,
            max_rollback_preference,
            warning_when_lagging,
            soku2_compatibility_mode: soku2_compat_mode,
            enable_println,
            enable_check_mode,
            turning_off_all_extra_ui,
            default_delay_takeover,
            outer_color,
            inside_color,
            progress_color,
            takeover_color,
            center_x_p1,
            center_y_p1,
            center_x_p2,
            center_y_p2,
            inside_half_height,
            inside_half_width,
            outer_half_height,
            outer_half_width,
            title,
        })
    }
}

fn read_ini_bool(
    conf: &HashMap<Identifier, Value>,
    section: &str,
    key: &str,
    default: bool,
) -> bool {
    conf.get(&Identifier::new(Some(section.to_string()), key.to_string()))
        .map(|x| match x {
            Value::Bool(x) => *x,
            _ => todo!("non bool .ini entry"),
        })
        .unwrap_or(default)
}

fn read_ini_int_hex(
    conf: &HashMap<Identifier, Value>,
    section: &str,
    key: &str,
    default: i64,
) -> i64 {
    conf.get(&Identifier::new(Some(section.to_string()), key.to_string()))
        .map(|x| match x {
            Value::Int(x) => *x,
            Value::Raw(x) | Value::Str(x) => {
                i64::from_str_radix(x.strip_prefix("0x").unwrap(), 16).unwrap()
            }
            _ => todo!("non integer .ini entry"),
        })
        .unwrap_or(default)
}

fn read_ini_string(
    conf: &HashMap<Identifier, Value>,
    section: &str,
    key: &str,
    default: String,
) -> String {
    conf.get(&Identifier::new(Some(section.to_string()), key.to_string()))
        .map(|x| match x {
            Value::Str(x) => x.clone(),
            _ => todo!("non string .ini entry"),
        })
        .unwrap_or(default)
}
