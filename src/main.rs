#![no_main]
#![no_std]
#![allow(static_mut_refs)]
#![allow(unsafe_op_in_unsafe_fn)]

extern crate flipperzero_rt;

use core::ffi::{CStr, c_char, c_void};
use core::ptr;
use core::sync::atomic::{AtomicBool, Ordering};

use flipperzero_rt::{entry, manifest};
use flipperzero_sys as sys;

mod dict;
mod solver;

use solver::Solver;

manifest!(
    name = "Wordle Solver",
    app_version = 1,
    has_icon = false,
);

static EXIT: AtomicBool = AtomicBool::new(false);
static NEED_FILTER: AtomicBool = AtomicBool::new(false);
static NEED_RESET: AtomicBool = AtomicBool::new(false);

static mut VP: *mut sys::ViewPort = ptr::null_mut();
static mut SOLVER: Solver = Solver::new();
static mut GUESS: u32 = 0;
static mut MARKS: [u8; 5] = [1; 5];
static mut CURSOR: usize = 0;
static mut SHOW_RESULT: bool = false;
static mut RESULT_TEXT: [u8; 32] = [0; 32];

fn u32_to_str(mut n: usize, buf: &mut [u8]) -> usize {
    let mut i = 0;
    let mut tmp = [0u8; 12];
    if n == 0 {
        tmp[0] = b'0';
        i = 1;
    } else {
        while n > 0 {
            tmp[i] = b'0' + (n % 10) as u8;
            i += 1;
            n /= 10;
        }
    }
    for j in 0..i {
        buf[j] = tmp[i - 1 - j];
    }
    i
}

unsafe fn draw_glyph(canvas: *mut sys::Canvas, x: i32, y: i32, letter_idx: usize) {
    let glyph = &dict::CYR_GLYPHS[letter_idx.min(32)];
    for row in 0..7 {
        let byte = glyph[row as usize];
        for col in 0..5 {
            if byte & (1 << (4 - col)) != 0 {
                sys::canvas_draw_dot(canvas, x + col, y + row);
            }
        }
    }
}

unsafe fn do_filter() {
    SOLVER.apply_feedback(GUESS, &MARKS);
    let count = SOLVER.count();
    if count <= 1 {
        if count == 1 {
            let idx = SOLVER.first_word().unwrap_or(0);
            GUESS = dict::get_word(idx);
            let txt = b"Guessed!\0";
            RESULT_TEXT[..txt.len()].copy_from_slice(txt);
        } else {
            let txt = b"No matches\0";
            RESULT_TEXT[..txt.len()].copy_from_slice(txt);
        }
        SHOW_RESULT = true;
    } else {
        let best = SOLVER.best_candidate();
        if let Some(idx) = best {
            GUESS = dict::get_word(idx);
        }
        CURSOR = 0;
        MARKS = [1; 5];
        SHOW_RESULT = false;
    }
}

unsafe fn do_reset() {
    SOLVER.init();
    let best = SOLVER.best_candidate();
    if let Some(idx) = best {
        GUESS = dict::get_word(idx);
    }
    CURSOR = 0;
    MARKS = [1; 5];
    SHOW_RESULT = false;
}

unsafe extern "C" fn draw_cb(canvas: *mut sys::Canvas, _ctx: *mut c_void) {
    sys::canvas_clear(canvas);
    sys::canvas_set_color(canvas, sys::ColorBlack);

    let guess = GUESS;
    let marks = MARKS;
    let cursor = CURSOR;
    let show_result = SHOW_RESULT;

    let letters = dict::unpack_word(guess);
    let count = SOLVER.count();

    sys::canvas_set_font(canvas, sys::FontSecondary);
    let mut buf = [0u8; 24];
    let prefix = b"Remaining: ";
    buf[..prefix.len()].copy_from_slice(prefix);
    let i = prefix.len() + u32_to_str(count, &mut buf[prefix.len()..]);
    buf[i] = 0;
    sys::canvas_draw_str(canvas, 2, 8, buf.as_ptr() as *const c_char);

    if show_result {
        // Render word using bitmap glyphs (proven to work)
        let guesses = dict::unpack_word(guess);
        let cell_w = 16i32;
        let cell_h = 16i32;
        let gap = 3i32;
        let total_w = 5 * cell_w + 4 * gap;
        let start_x = (128 - total_w) / 2;
        let cell_y = 20i32;
        for i in 0..5 {
            let x = start_x + i as i32 * (cell_w + gap);
            let li = guesses[i] as usize;
            sys::canvas_set_color(canvas, sys::ColorBlack);
            sys::canvas_draw_frame(canvas, x, cell_y, cell_w as usize, cell_h as usize);
            draw_glyph(canvas, x + 5, cell_y + 4, li);
        }

        // Result label
        sys::canvas_set_font(canvas, sys::FontSecondary);
        let rp = RESULT_TEXT.as_ptr();
        sys::canvas_draw_str(canvas, 2, 55, rp as *const c_char);

        let hint = c"Back - restart";
        sys::canvas_draw_str(canvas, 2, 63, hint.as_ptr());
        return;
    }

    let cell_w = 16i32;
    let cell_h = 16i32;
    let gap = 3i32;
    let total_w = 5 * cell_w + 4 * gap;
    let start_x = (128 - total_w) / 2;
    let cell_y = 16i32;

    for i in 0..5 {
        let x = start_x + i as i32 * (cell_w + gap);
        let mark = marks[i];
        let li = letters[i] as usize;

        match mark {
            1 => {
                sys::canvas_set_color(canvas, sys::ColorBlack);
                sys::canvas_draw_box(canvas, x, cell_y, cell_w as usize, cell_h as usize);
                sys::canvas_set_color(canvas, sys::ColorWhite);
                draw_glyph(canvas, x + 5, cell_y + 4, li);
                sys::canvas_set_color(canvas, sys::ColorBlack);
            }
            2 => {
                sys::canvas_set_color(canvas, sys::ColorBlack);
                sys::canvas_draw_box(canvas, x, cell_y, cell_w as usize, cell_h as usize);
                sys::canvas_set_color(canvas, sys::ColorWhite);
                sys::canvas_draw_box(canvas, x + 2, cell_y + 2, (cell_w - 4) as usize, (cell_h - 4) as usize);
                sys::canvas_set_color(canvas, sys::ColorBlack);
                draw_glyph(canvas, x + 5, cell_y + 4, li);
            }
            3 => {
                sys::canvas_set_color(canvas, sys::ColorBlack);
                sys::canvas_draw_frame(canvas, x, cell_y, cell_w as usize, cell_h as usize);
                draw_glyph(canvas, x + 5, cell_y + 4, li);
            }
            _ => {}
        }

        if i == cursor {
            sys::canvas_set_color(canvas, sys::ColorBlack);
            sys::canvas_draw_box(canvas, x, cell_y + cell_h + 1, cell_w as usize, 2usize);
        }
    }

    sys::canvas_set_font(canvas, sys::FontSecondary);
    let instr = c"^v mark  <> cursor  OK apply";
    sys::canvas_draw_str(canvas, 2, 55, instr.as_ptr());
    let instr2 = c"Back reset  LongBack exit";
    sys::canvas_draw_str(canvas, 2, 63, instr2.as_ptr());
}

unsafe extern "C" fn input_cb(event: *mut sys::InputEvent, _ctx: *mut c_void) {
    let ev = &*event;

    if ev.key == sys::InputKeyBack && ev.type_ == sys::InputTypeLong {
        EXIT.store(true, Ordering::SeqCst);
        return;
    }

    if ev.type_ != sys::InputTypePress {
        return;
    }

    if SHOW_RESULT {
        if ev.key == sys::InputKeyBack {
            NEED_RESET.store(true, Ordering::SeqCst);
        }
        sys::view_port_update(VP);
        return;
    }

    match ev.key {
        sys::InputKeyUp => {
            let m = MARKS[CURSOR];
            MARKS[CURSOR] = if m >= 3 { 1 } else { m + 1 };
        }
        sys::InputKeyDown => {
            let m = MARKS[CURSOR];
            MARKS[CURSOR] = if m <= 1 { 3 } else { m - 1 };
        }
        sys::InputKeyLeft => {
            if CURSOR > 0 {
                CURSOR -= 1;
            }
        }
        sys::InputKeyRight => {
            if CURSOR < 4 {
                CURSOR += 1;
            }
        }
        sys::InputKeyOk => {
            NEED_FILTER.store(true, Ordering::SeqCst);
        }
        sys::InputKeyBack => {
            NEED_RESET.store(true, Ordering::SeqCst);
        }
        _ => {}
    }

    sys::view_port_update(VP);
}

entry!(main);

fn main(_args: Option<&CStr>) -> i32 {
    dict::init_storage();

    unsafe {
        SOLVER = Solver::new();
        SOLVER.init();
        let best = SOLVER.best_candidate();
        if let Some(idx) = best {
            GUESS = dict::get_word(idx);
        }
        MARKS = [1; 5];
        CURSOR = 0;
        SHOW_RESULT = false;

        let vp = sys::view_port_alloc();
        VP = vp;
        sys::view_port_draw_callback_set(vp, Some(draw_cb), ptr::null_mut());
        sys::view_port_input_callback_set(vp, Some(input_cb), ptr::null_mut());
        let gui = sys::furi::UnsafeRecord::open(c"gui");
        sys::gui_add_view_port(gui.as_ptr(), vp, sys::GuiLayerFullscreen);
        sys::view_port_update(vp);

        while !EXIT.load(Ordering::SeqCst) {
            if NEED_FILTER.load(Ordering::SeqCst) {
                NEED_FILTER.store(false, Ordering::SeqCst);
                do_filter();
                sys::view_port_update(VP);
            }
            if NEED_RESET.load(Ordering::SeqCst) {
                NEED_RESET.store(false, Ordering::SeqCst);
                do_reset();
                sys::view_port_update(VP);
            }
            sys::furi_delay_ms(100);
        }

        dict::close_storage();
        sys::view_port_enabled_set(vp, false);
        sys::gui_remove_view_port(gui.as_ptr(), vp);
        sys::view_port_free(vp);
        VP = ptr::null_mut();
    }
    0
}
