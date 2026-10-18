//! The displays macOS currently draws on, and switching one off and on again.
//!
//! Switching off uses the same private CoreGraphics call as "disconnect display" in other display
//! tools: macOS behaves as if the cable was pulled, moves windows, menu bar and Dock to the
//! remaining screens and stops sending a picture, so the monitor can show another input. The
//! change lasts until it is undone or the user logs out.

use std::ffi::c_void;

use objc2::MainThreadMarker;
use objc2_app_kit::NSScreen;
use objc2_foundation::{NSNumber, ns_string};

#[repr(C)]
#[derive(Debug, Clone, Copy)]
struct CGPoint {
    x: f64,
    y: f64,
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
struct CGSize {
    width: f64,
    height: f64,
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
struct CGRect {
    origin: CGPoint,
    size: CGSize,
}

type ConfigRef = *mut c_void;
type ReconfigurationCallback = extern "C" fn(display: u32, flags: u32, user_info: *mut c_void);

/// `kCGConfigureForSession`: undone at logout at the latest.
const CONFIGURE_FOR_SESSION: u32 = 1;
/// `kCGDisplayBeginConfigurationFlag`: the change is about to happen.
const BEGIN_CONFIGURATION: u32 = 1;

#[link(name = "CoreGraphics", kind = "framework")]
unsafe extern "C" {
    fn CGGetActiveDisplayList(max: u32, ids: *mut u32, count: *mut u32) -> i32;
    fn CGDisplayBounds(display: u32) -> CGRect;
    fn CGMainDisplayID() -> u32;
    fn CGDisplayIsBuiltin(display: u32) -> bool;
    fn CGBeginDisplayConfiguration(config: *mut ConfigRef) -> i32;
    fn CGCompleteDisplayConfiguration(config: ConfigRef, option: u32) -> i32;
    fn CGCancelDisplayConfiguration(config: ConfigRef) -> i32;
    fn CGDisplayRegisterReconfigurationCallback(callback: ReconfigurationCallback, user_info: *mut c_void) -> i32;
}

type ConfigureEnabled = extern "C" fn(config: ConfigRef, display: u32, enabled: bool) -> i32;

/// `CGSConfigureDisplayEnabled`: private, but used by display tools for many years. Looked up at
/// run time, so a macOS without it only loses the function instead of not starting.
fn configure_enabled() -> Option<ConfigureEnabled> {
    let symbol = unsafe { libc::dlsym(libc::RTLD_DEFAULT, c"CGSConfigureDisplayEnabled".as_ptr()) };
    (!symbol.is_null()).then(|| unsafe { std::mem::transmute::<*mut c_void, ConfigureEnabled>(symbol) })
}

/// This macOS can switch displays off.
pub fn supported() -> bool {
    configure_enabled().is_some()
}

#[derive(Debug, Clone, PartialEq)]
pub struct Display {
    pub id: u32,
    /// Left edge in the arrangement of System Settings (points).
    pub x: f64,
    pub builtin: bool,
    pub main: bool,
    /// What the monitor calls itself ("LEN L28u-30"), empty if unknown.
    pub name: String,
}

/// The displays with a picture right now.
pub fn active() -> Vec<Display> {
    let mut ids = [0u32; 32];
    let mut count = 0u32;
    if unsafe { CGGetActiveDisplayList(ids.len() as u32, ids.as_mut_ptr(), &mut count) } != 0 {
        return Vec::new();
    }
    let main = unsafe { CGMainDisplayID() };
    let names = names();
    ids[..count as usize]
        .iter()
        .map(|&id| Display {
            id,
            x: unsafe { CGDisplayBounds(id) }.origin.x,
            builtin: unsafe { CGDisplayIsBuiltin(id) },
            main: id == main,
            name: names
                .iter()
                .find(|(n, _)| *n == id)
                .map(|(_, name)| name.clone())
                .unwrap_or_default(),
        })
        .collect()
}

/// The names of the screens as System Settings shows them (only known on the main thread).
fn names() -> Vec<(u32, String)> {
    let Some(mtm) = MainThreadMarker::new() else {
        return Vec::new();
    };
    NSScreen::screens(mtm)
        .iter()
        .filter_map(|screen| {
            let number = screen.deviceDescription().objectForKey(ns_string!("NSScreenNumber"))?;
            let number = number.downcast::<NSNumber>().ok()?;
            Some((number.unsignedIntValue(), screen.localizedName().to_string()))
        })
        .collect()
}

/// Switches displays off or on in one step. The error is the CoreGraphics code (-1: not supported).
pub fn set_enabled(ids: &[u32], enabled: bool) -> Result<(), i32> {
    if ids.is_empty() {
        return Ok(());
    }
    let configure = configure_enabled().ok_or(-1)?;
    unsafe {
        let mut config: ConfigRef = std::ptr::null_mut();
        let err = CGBeginDisplayConfiguration(&mut config);
        if err != 0 {
            return Err(err);
        }
        for &id in ids {
            let err = configure(config, id, enabled);
            if err != 0 {
                CGCancelDisplayConfiguration(config);
                return Err(err);
            }
        }
        match CGCompleteDisplayConfiguration(config, CONFIGURE_FOR_SESSION) {
            0 => Ok(()),
            err => Err(err),
        }
    }
}

/// Calls `on_change` on the main thread after displays came, went or moved.
pub fn watch(on_change: fn()) {
    extern "C" fn callback(_display: u32, flags: u32, user_info: *mut c_void) {
        if flags & BEGIN_CONFIGURATION != 0 {
            return;
        }
        let on_change: fn() = unsafe { std::mem::transmute(user_info) };
        on_change();
    }
    unsafe { CGDisplayRegisterReconfigurationCallback(callback, on_change as *mut c_void) };
}
