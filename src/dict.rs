#![allow(dead_code)]

include!(concat!(env!("OUT_DIR"), "/dict_data.rs"));

use core::ffi::{c_void, c_char};
use core::ptr;
use flipperzero_sys as sys;

const DICT_PATH: &str = "/ext/apps/Wordle/dict.bin\0";

// Cached storage handle — opened once at startup, never closed
static mut STORAGE: *mut sys::Storage = ptr::null_mut();

pub fn init_storage() {
    unsafe {
        let rec = sys::furi::UnsafeRecord::open(c"storage");
        STORAGE = rec.as_ptr();
        // Leak the UnsafeRecord — keep storage open for app lifetime
        core::mem::forget(rec);
    }
}

fn with_file<F: FnOnce(*mut sys::File)>(f: F) {
    unsafe {
        let s = STORAGE;
        if s.is_null() {
            return;
        }
        let file = sys::storage_file_alloc(s);
        let opened = sys::storage_file_open(
            file,
            DICT_PATH.as_ptr() as *const c_char,
            sys::FSAM_READ,
            sys::FSOM_OPEN_EXISTING,
        );
        if opened {
            f(file);
            sys::storage_file_close(file);
        }
        sys::storage_file_free(file);
    }
}

pub fn for_each_word(mut f: impl FnMut(u16, u32)) {
    with_file(|file| unsafe {
        sys::storage_file_seek(file, 4, true);
        let mut buf = [0u8; 4];
        for i in 0..WORD_COUNT as u16 {
            let nread = sys::storage_file_read(file, buf.as_mut_ptr() as *mut c_void, 4);
            if nread != 4 {
                break;
            }
            let word = u32::from_le_bytes(buf);
            f(i, word);
        }
    });
}

pub fn get_word(idx: u16) -> u32 {
    let mut word = 0u32;
    with_file(|file| unsafe {
        let offset = 4u32 + (idx as u32) * 4;
        sys::storage_file_seek(file, offset, true);
        let mut buf = [0u8; 4];
        sys::storage_file_read(file, buf.as_mut_ptr() as *mut c_void, 4);
        word = u32::from_le_bytes(buf);
    });
    word
}

pub fn unpack_word(packed: u32) -> [u8; 5] {
    let mut letters = [0u8; 5];
    for i in 0..5 {
        letters[i] = ((packed >> (i * 6)) & 0x3F) as u8;
    }
    letters
}

pub fn unique_count(word: u32) -> u32 {
    let mut mask = 0u32;
    for i in 0..5 {
        let letter = ((word >> (i * 6)) & 0x3F) as u8;
        mask |= 1u32 << letter;
    }
    mask.count_ones()
}

pub fn word_matches(word: u32, guess: u32, marks: &[u8; 5]) -> bool {
    let w = unpack_word(word);
    let g = unpack_word(guess);

    let mut used = [false; 5];

    for i in 0..5 {
        if marks[i] == 1 {
            if w[i] != g[i] {
                return false;
            }
            used[i] = true;
        }
    }

    for i in 0..5 {
        if marks[i] == 2 {
            if w[i] == g[i] {
                return false;
            }
            let mut found = false;
            for j in 0..5 {
                if !used[j] && w[j] == g[i] {
                    used[j] = true;
                    found = true;
                    break;
                }
            }
            if !found {
                return false;
            }
        }
    }

    for i in 0..5 {
        if marks[i] == 3 {
            for j in 0..5 {
                if !used[j] && w[j] == g[i] {
                    return false;
                }
            }
        }
    }

    true
}

pub fn word_to_str(word: u32) -> [u8; 11] {
    let letters = unpack_word(word);
    let mut buf = [0u8; 11];
    for i in 0..5 {
        let idx = letters[i] as usize;
        let bytes = &LETTER_BYTES[idx];
        buf[i * 2] = bytes[0];
        buf[i * 2 + 1] = bytes[1];
    }
    buf[10] = 0;
    buf
}

/// 5x7 pixel bitmap glyphs for all 33 Russian letters (pixels right-aligned: bit0=leftmost)
const CYR_GLYPHS: [[u8; 7]; 33] = [
    [0x04, 0x0A, 0x11, 0x11, 0x1F, 0x11, 0x11], // А
    [0x1F, 0x10, 0x10, 0x1E, 0x11, 0x11, 0x1E], // Б
    [0x1E, 0x11, 0x11, 0x1E, 0x11, 0x11, 0x1E], // В
    [0x1F, 0x10, 0x10, 0x10, 0x10, 0x10, 0x10], // Г
    [0x04, 0x04, 0x0A, 0x0A, 0x11, 0x11, 0x1F], // Д
    [0x1F, 0x10, 0x10, 0x1E, 0x10, 0x10, 0x1F], // Е
    [0x1F, 0x10, 0x10, 0x1E, 0x10, 0x10, 0x1F], // Ё
    [0x11, 0x0A, 0x0A, 0x04, 0x0A, 0x0A, 0x11], // Ж
    [0x1E, 0x01, 0x01, 0x06, 0x01, 0x01, 0x1E], // З
    [0x11, 0x11, 0x13, 0x15, 0x19, 0x11, 0x11], // И
    [0x11, 0x11, 0x13, 0x15, 0x19, 0x11, 0x11], // Й
    [0x11, 0x12, 0x14, 0x18, 0x14, 0x12, 0x11], // К
    [0x01, 0x01, 0x03, 0x05, 0x09, 0x11, 0x11], // Л
    [0x11, 0x1B, 0x15, 0x11, 0x11, 0x11, 0x11], // М
    [0x11, 0x11, 0x11, 0x1F, 0x11, 0x11, 0x11], // Н
    [0x0E, 0x11, 0x11, 0x11, 0x11, 0x11, 0x0E], // О
    [0x1F, 0x11, 0x11, 0x11, 0x11, 0x11, 0x11], // П
    [0x1E, 0x11, 0x11, 0x1E, 0x10, 0x10, 0x10], // Р
    [0x0E, 0x11, 0x10, 0x10, 0x10, 0x11, 0x0E], // С
    [0x1F, 0x04, 0x04, 0x04, 0x04, 0x04, 0x04], // Т
    [0x11, 0x11, 0x11, 0x0A, 0x0A, 0x04, 0x04], // У
    [0x04, 0x0E, 0x1F, 0x15, 0x15, 0x15, 0x0E], // Ф
    [0x11, 0x11, 0x0A, 0x04, 0x0A, 0x11, 0x11], // Х
    [0x11, 0x11, 0x11, 0x11, 0x11, 0x1F, 0x01], // Ц
    [0x11, 0x11, 0x11, 0x0F, 0x01, 0x01, 0x01], // Ч
    [0x15, 0x15, 0x15, 0x15, 0x15, 0x15, 0x1F], // Ш
    [0x15, 0x15, 0x15, 0x15, 0x15, 0x1F, 0x01], // Щ
    [0x1E, 0x02, 0x02, 0x06, 0x03, 0x02, 0x02], // Ъ
    [0x1E, 0x12, 0x12, 0x1E, 0x12, 0x12, 0x12], // Ы
    [0x1E, 0x10, 0x10, 0x1E, 0x11, 0x11, 0x10], // Ь
    [0x0E, 0x11, 0x01, 0x07, 0x01, 0x11, 0x0E], // Э
    [0x12, 0x12, 0x12, 0x1E, 0x12, 0x12, 0x12], // Ю
    [0x0F, 0x11, 0x11, 0x0F, 0x05, 0x09, 0x11], // Я
];

pub fn draw_cyr_word(canvas: *mut sys::Canvas, x_pos: &[i32; 5], y: i32, word: u32) {
    let letters = unpack_word(word);
    for li in 0..5 {
        let glyph = &CYR_GLYPHS[letters[li] as usize];
        let ox = x_pos[li];
        for row in 0..7 {
            let byte = glyph[row as usize];
            for col in 0..5 {
                if byte & (1 << (4 - col)) != 0 {
                    unsafe {
                        sys::canvas_draw_dot(canvas, ox + col, y + row);
                    }
                }
            }
        }
    }
}

pub const LETTER_BYTES: [[u8; 3]; 33] = [
    [0xD0, 0x90, 0], // А
    [0xD0, 0x91, 0], // Б
    [0xD0, 0x92, 0], // В
    [0xD0, 0x93, 0], // Г
    [0xD0, 0x94, 0], // Д
    [0xD0, 0x95, 0], // Е
    [0xD0, 0x81, 0], // Ё
    [0xD0, 0x96, 0], // Ж
    [0xD0, 0x97, 0], // З
    [0xD0, 0x98, 0], // И
    [0xD0, 0x99, 0], // Й
    [0xD0, 0x9A, 0], // К
    [0xD0, 0x9B, 0], // Л
    [0xD0, 0x9C, 0], // М
    [0xD0, 0x9D, 0], // Н
    [0xD0, 0x9E, 0], // О
    [0xD0, 0x9F, 0], // П
    [0xD0, 0xA0, 0], // Р
    [0xD0, 0xA1, 0], // С
    [0xD0, 0xA2, 0], // Т
    [0xD0, 0xA3, 0], // У
    [0xD0, 0xA4, 0], // Ф
    [0xD0, 0xA5, 0], // Х
    [0xD0, 0xA6, 0], // Ц
    [0xD0, 0xA7, 0], // Ч
    [0xD0, 0xA8, 0], // Ш
    [0xD0, 0xA9, 0], // Щ
    [0xD0, 0xAA, 0], // Ъ
    [0xD0, 0xAB, 0], // Ы
    [0xD0, 0xAC, 0], // Ь
    [0xD0, 0xAD, 0], // Э
    [0xD0, 0xAE, 0], // Ю
    [0xD0, 0xAF, 0], // Я
];
