//! Input handling for reading player inputs from keyboard and controller.

use crate::{ptr_wrap, INPUT_KEYS_NUMBERS};

/// Read a key's pressed state from the raw input buffer.
#[inline]
pub unsafe fn read_key_better(key: u8) -> bool {
    const RAW_INPUT_BUFFER: u32 = 0x8a01b8;
    *((RAW_INPUT_BUFFER + key as u32) as *const u8) != 0
}

/// Read current input state from keyboard or controller.
pub unsafe fn read_current_input() -> [bool; INPUT_KEYS_NUMBERS] {
    const LOCAL_INPUT_MANAGER: u32 = 0x898938;
    const RAW_INPUT_BUFFER: u32 = 0x8a01b8;
    let mut input = [false; INPUT_KEYS_NUMBERS];

    let controller_id = *((LOCAL_INPUT_MANAGER + 0x4) as *const u8);

    if controller_id == 255 {
        // No controllers, reading keyboard input
        for a in 0..INPUT_KEYS_NUMBERS {
            let key = (LOCAL_INPUT_MANAGER + 0x8 + a as u32 * 0x4) as *const u8;
            let key = *key as u32;
            let key = *((RAW_INPUT_BUFFER + key) as *const u8) != 0;
            input[a] = key;
        }
    } else {
        let get_controller =
            std::mem::transmute::<usize, extern "thiscall" fn(u32, u32) -> u32>(0x40dc60);
        let controller = get_controller(0x8a0198, controller_id as u32);

        if controller != 0 {
            let axis1 = *ptr_wrap!(controller as *const i32);
            let axis2 = *ptr_wrap!((controller + 4) as *const i32);

            input[2] = axis1 < -500;
            input[3] = axis1 > 500;

            input[0] = axis2 < -500;
            input[1] = axis2 > 500;

            for a in 0..(INPUT_KEYS_NUMBERS - 4) {
                let key = *ptr_wrap!((LOCAL_INPUT_MANAGER + 0x18 + a as u32 * 0x4) as *const i32);

                if key > -1 {
                    input[a + 4] = *ptr_wrap!((key as u32 + 0x30 + controller) as *const u8) != 0;
                }
            }
        }
    }

    input
}

/// Convert input array to accumulated u16 bitfield.
#[inline]
pub fn input_to_accum(inp: &[bool; INPUT_KEYS_NUMBERS]) -> u16 {
    let mut accum = 0u16;
    for a in 0..INPUT_KEYS_NUMBERS {
        if inp[a] {
            accum |= 1 << a;
        }
    }
    accum
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_input_to_accum_empty() {
        let input = [false; INPUT_KEYS_NUMBERS];
        assert_eq!(input_to_accum(&input), 0);
    }

    #[test]
    fn test_input_to_accum_first_bit() {
        let mut input = [false; INPUT_KEYS_NUMBERS];
        input[0] = true;
        assert_eq!(input_to_accum(&input), 1);
    }

    #[test]
    fn test_input_to_accum_multiple_bits() {
        let mut input = [false; INPUT_KEYS_NUMBERS];
        input[0] = true;
        input[2] = true;
        input[4] = true;
        assert_eq!(input_to_accum(&input), 0b10101);
    }
}
