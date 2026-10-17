//! Running code on the main thread later, through the main dispatch queue of macOS. Menu clicks,
//! display changes, signals and background work all end up here, one after the other, so the app
//! state needs no locks and is never used twice at the same time.

use std::ffi::c_void;
use std::time::Duration;

#[repr(C)]
struct Opaque {
    _private: [u8; 0],
}

const DISPATCH_TIME_NOW: u64 = 0;

unsafe extern "C" {
    static _dispatch_main_q: Opaque;
    static _dispatch_source_type_signal: Opaque;
    fn dispatch_time(when: u64, delta: i64) -> u64;
    fn dispatch_after_f(when: u64, queue: *const c_void, context: *mut c_void, work: extern "C" fn(*mut c_void));
    fn dispatch_source_create(kind: *const c_void, handle: usize, mask: usize, queue: *const c_void) -> *mut c_void;
    fn dispatch_source_set_event_handler_f(source: *mut c_void, handler: extern "C" fn(*mut c_void));
    fn dispatch_set_context(object: *mut c_void, context: *mut c_void);
    fn dispatch_resume(object: *mut c_void);
}

type Job = Box<dyn FnOnce() + Send>;

fn main_queue() -> *const c_void {
    (&raw const _dispatch_main_q).cast()
}

/// Runs `job` on the main thread after `delay`.
pub fn after(delay: Duration, job: impl FnOnce() + Send + 'static) {
    extern "C" fn run(context: *mut c_void) {
        let job = unsafe { Box::from_raw(context.cast::<Job>()) };
        job();
    }
    let job: Box<Job> = Box::new(Box::new(job));
    let nanos = i64::try_from(delay.as_nanos()).unwrap_or(i64::MAX);
    unsafe {
        dispatch_after_f(
            dispatch_time(DISPATCH_TIME_NOW, nanos),
            main_queue(),
            Box::into_raw(job).cast(),
            run,
        );
    }
}

/// Runs `job` on the main thread as soon as it is free.
pub fn later(job: impl FnOnce() + Send + 'static) {
    after(Duration::ZERO, job);
}

/// Calls `handler` on the main thread when the process gets `signal` (instead of ending it).
pub fn on_signal(signal: i32, handler: fn()) {
    extern "C" fn run(context: *mut c_void) {
        let handler: fn() = unsafe { std::mem::transmute(context) };
        handler();
    }
    unsafe {
        libc::signal(signal, libc::SIG_IGN);
        let kind = (&raw const _dispatch_source_type_signal).cast();
        let source = dispatch_source_create(kind, signal as usize, 0, main_queue());
        if source.is_null() {
            return;
        }
        // Lives as long as the process
        dispatch_set_context(source, handler as *mut c_void);
        dispatch_source_set_event_handler_f(source, run);
        dispatch_resume(source);
    }
}
