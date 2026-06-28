#![no_main]
#![no_std]
#![allow(static_mut_refs)]

extern crate flipperzero_rt;

mod dict;
mod solver;

use core::ffi::{CStr, c_char, c_void};
use core::ptr;
use core::sync::atomic::{AtomicBool, Ordering};
use core::time::Duration;

use flipperzero::furi::thread::sleep;
use flipperzero_rt::{entry, manifest};
use flipperzero_sys as sys;

use solver::Solver;

manifest!(
    name = "Wordle Solver",
    app_version = 1,
    has_icon = false,
);

const MARK_NONE: u8 = 0;
const MARK_GREEN: u8 = 1;
const MARK_YELLOW: u8 = 2;
const MARK_GRAY: u8 = 3;

const LET_X: [i32; 5] = [8, 32, 56, 80, 104];

struct AppState {
    solver: Solver,
    suggested_idx: u16,
    suggested_word: u32,
    cursor: u8,
    marks: [u8; 5],
    phase: Phase,
}

#[derive(Clone, Copy, PartialEq)]
enum Phase {
    Marking,
    Result,
}

static mut APP: AppState = AppState {
    solver: Solver::new(),
    suggested_idx: 0,
    suggested_word: 0,
    cursor: 0,
    marks: [0; 5],
    phase: Phase::Marking,
};

static mut APP_MUTEX: *mut sys::FuriMutex = ptr::null_mut();
static mut VIEW_PORT: *mut sys::ViewPort = ptr::null_mut();
static EXIT: AtomicBool = AtomicBool::new(false);
static REDRAW: AtomicBool = AtomicBool::new(false);

fn lock_app() {
    unsafe { sys::furi_mutex_acquire(APP_MUTEX, u32::MAX); }
}

fn unlock_app() {
    unsafe { sys::furi_mutex_release(APP_MUTEX); }
}

fn fmt_u16(mut n: u16, buf: &mut [u8; 6]) -> *const c_char {
    if n == 0 {
        buf[0] = b'0';
        buf[1] = 0;
        return buf.as_ptr() as *const c_char;
    }
    let mut i = 0i32;
    while n > 0 && i < 5 {
        buf[i as usize] = b'0' + (n % 10) as u8;
        n /= 10;
        i += 1;
    }
    buf[i as usize] = 0;
    let len = i as usize;
    for j in 0..len / 2 {
        let tmp = buf[j];
        buf[j] = buf[len - 1 - j];
        buf[len - 1 - j] = tmp;
    }
    buf.as_ptr() as *const c_char
}

fn show_str(canvas: *mut sys::Canvas, x: i32, y: i32, s: &CStr) {
    unsafe {
        sys::canvas_draw_str(canvas, x, y, s.as_ptr());
    }
}

fn show_marks(canvas: *mut sys::Canvas, marks: &[u8; 5], y: i32) {
    unsafe {
        sys::canvas_set_font(canvas, sys::FontSecondary);
    }
    for i in 0..5 {
        let x = LET_X[i];
        match marks[i] {
            MARK_GREEN => show_str(canvas, x + 2, y, c"G"),
            MARK_YELLOW => show_str(canvas, x + 2, y, c"Y"),
            MARK_GRAY => show_str(canvas, x + 2, y, c"X"),
            _ => {}
        }
    }
}

unsafe extern "C" fn draw_callback(canvas: *mut sys::Canvas, _ctx: *mut c_void) {
    lock_app();
    unsafe {
        sys::canvas_clear(canvas);
        sys::canvas_set_font(canvas, sys::FontSecondary);
    }
    show_str(canvas, 2, 9, c"WORDLE SOLVER");

    let word_packed = unsafe { APP.suggested_word };
    dict::draw_cyr_word(canvas, &LET_X, 20, word_packed);

    match unsafe { APP.phase } {
        Phase::Marking => {
            let cx = LET_X[unsafe { APP.cursor } as usize];
            unsafe {
                sys::canvas_draw_line(canvas, cx, 30, cx + 16, 30);
                sys::canvas_draw_line(canvas, cx, 31, cx + 16, 31);
            }

            show_marks(canvas, unsafe { &APP.marks }, 46);

            unsafe { sys::canvas_set_font(canvas, sys::FontSecondary); }
            let mut nbuf = [0u8; 6];
            let num = fmt_u16(unsafe { APP.solver.count() as u16 }, &mut nbuf);
            show_str(canvas, 2, 56, c"Left:");
            unsafe { sys::canvas_draw_str(canvas, 32, 56, num); }

            show_str(canvas, 2, 63, c"OK=apply <-=reset");
        }

        Phase::Result => {
            let count = unsafe { APP.solver.count() };
            unsafe { sys::canvas_set_font(canvas, sys::FontSecondary); }

            if count == 0 {
                show_str(canvas, 2, 48, c"NOT FOUND!");
            } else {
                let mut nbuf = [0u8; 6];
                let num = fmt_u16(count as u16, &mut nbuf);
                show_str(canvas, 2, 48, c"Left:");
                unsafe { sys::canvas_draw_str(canvas, 32, 48, num); }
            }

            show_str(canvas, 2, 58, c"OK=restart  L..B=exit");
        }
    }

    unlock_app();
}

unsafe extern "C" fn input_callback(event: *mut sys::InputEvent, _ctx: *mut c_void) {
    let ev = unsafe { &*event };

    if ev.key == sys::InputKeyBack && ev.type_ == sys::InputTypeLong {
        EXIT.store(true, Ordering::SeqCst);
        return;
    }

    lock_app();

    let do_redraw = unsafe { match APP.phase {
        Phase::Marking => match ev.key {
            sys::InputKeyLeft if ev.type_ == sys::InputTypePress => {
                APP.cursor = if APP.cursor == 0 { 4 } else { APP.cursor - 1 };
                true
            }

            sys::InputKeyRight if ev.type_ == sys::InputTypePress => {
                APP.cursor = if APP.cursor == 4 { 0 } else { APP.cursor + 1 };
                true
            }

            sys::InputKeyUp if ev.type_ == sys::InputTypePress => {
                let i = APP.cursor as usize;
                APP.marks[i] = match APP.marks[i] {
                    MARK_NONE => MARK_GREEN,
                    MARK_GREEN => MARK_YELLOW,
                    MARK_YELLOW => MARK_GRAY,
                    _ => MARK_NONE,
                };
                true
            }

            sys::InputKeyDown if ev.type_ == sys::InputTypePress => {
                let i = APP.cursor as usize;
                APP.marks[i] = match APP.marks[i] {
                    MARK_NONE => MARK_GRAY,
                    MARK_GRAY => MARK_YELLOW,
                    MARK_YELLOW => MARK_GREEN,
                    _ => MARK_NONE,
                };
                true
            }

            sys::InputKeyOk if ev.type_ == sys::InputTypePress => {
                let guess_word = APP.suggested_word;
                let marks = APP.marks;
                APP.solver.apply_feedback(guess_word, &marks);

                let remaining = APP.solver.count();
                if remaining <= 1 {
                    APP.phase = Phase::Result;
                    if remaining == 1 {
                        APP.suggested_idx = APP.solver.first_word().unwrap_or(0);
                        APP.suggested_word = dict::get_word(APP.suggested_idx);
                    }
                } else if let Some(next) = APP.solver.best_candidate() {
                    APP.suggested_idx = next;
                    APP.suggested_word = dict::get_word(next);
                    APP.cursor = 0;
                    APP.marks = [0; 5];
                }
                true
            }

            sys::InputKeyBack if ev.type_ == sys::InputTypeShort => {
                APP.marks = [0; 5];
                APP.cursor = 0;
                true
            }

            _ => false,
        },

        Phase::Result => {
            if ev.key == sys::InputKeyOk && ev.type_ == sys::InputTypePress {
                APP.solver.init();
                APP.phase = Phase::Marking;
                APP.cursor = 0;
                APP.marks = [0; 5];
                let idx = dict::BEST_OPENERS
                    .first()
                    .copied()
                    .or_else(|| APP.solver.best_candidate())
                    .unwrap_or(0);
                APP.suggested_idx = idx;
                APP.suggested_word = dict::get_word(idx);
                true
            } else {
                false
            }
        }
    } };

    unlock_app();

    if do_redraw {
        REDRAW.store(true, Ordering::SeqCst);
    }
}

fn redraw() {
    unsafe {
        if !VIEW_PORT.is_null() {
            sys::view_port_update(VIEW_PORT);
        }
    }
}

entry!(main);

fn main(_args: Option<&CStr>) -> i32 {
    dict::init_storage();

    unsafe {
        APP_MUTEX = sys::furi_mutex_alloc(sys::FuriMutexTypeNormal);
    }

    lock_app();
    unsafe {
        APP.solver.init();
    }
    let idx = dict::BEST_OPENERS
        .first()
        .copied()
        .or_else(|| unsafe { APP.solver.best_candidate() })
        .unwrap_or(0);
    unsafe {
        APP.suggested_idx = idx;
        APP.suggested_word = dict::get_word(idx);
    }
    unlock_app();

    unsafe {
        let view_port = sys::view_port_alloc();
        VIEW_PORT = view_port;

        sys::view_port_draw_callback_set(view_port, Some(draw_callback), ptr::null_mut());
        sys::view_port_input_callback_set(view_port, Some(input_callback), ptr::null_mut());

        let gui = sys::furi::UnsafeRecord::open(c"gui");
        sys::gui_add_view_port(gui.as_ptr(), view_port, sys::GuiLayerFullscreen);

        redraw();

        while !EXIT.load(Ordering::SeqCst) {
            if REDRAW.swap(false, Ordering::SeqCst) {
                redraw();
            }
            sleep(Duration::from_millis(50));
        }

        sys::view_port_enabled_set(view_port, false);
        sys::gui_remove_view_port(gui.as_ptr(), view_port);
        sys::view_port_free(view_port);
        VIEW_PORT = ptr::null_mut();
    }

    0
}
