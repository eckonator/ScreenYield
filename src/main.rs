//! ScreenYield: a menu bar app for macOS that switches external monitors off and on again, so
//! another computer connected to the same monitors can have them.

mod autostart;
mod dispatch;
mod displays;
mod i18n;
mod icon;
mod log;
mod menu;
mod model;
mod self_update;
mod settings;
mod update;

use std::os::fd::AsRawFd;
use std::path::Path;

use objc2::MainThreadMarker;
use objc2_app_kit::{NSApplication, NSApplicationActivationPolicy};

/// "0.1.0", for test builds with the commit ("0.1.0 (test 1a2b3c4)").
pub fn version() -> String {
    let version = env!("CARGO_PKG_VERSION");
    let commit = env!("SY_GIT_HASH");
    if update::is_release() || commit.is_empty() {
        version.to_string()
    } else {
        format!("{version} (test {})", &commit[..commit.len().min(7)])
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if let Some(i) = args.iter().position(|a| a == "--write-icons") {
        let dir = args.get(i + 1).map_or(".", String::as_str);
        if let Err(e) = icon::write_iconset(Path::new(dir)) {
            eprintln!("icons not written: {e}");
            std::process::exit(1);
        }
        return;
    }
    if args.iter().any(|a| a == "--version") {
        println!("ScreenYield {}", version());
        return;
    }

    log::init();
    let Some(mtm) = MainThreadMarker::new() else {
        log!("not started on the main thread");
        return;
    };
    if !single_instance() {
        log!("ScreenYield runs already, this start ends");
        return;
    }
    log!("ScreenYield {} starts", version());
    if !displays::supported() {
        log!("this macOS cannot switch displays off");
    }
    // After a crash, a forced quit or an update: everything on again
    restore_all();
    // Quit from the menu, logout (NSApplication ends with exit) and signals: the same
    unsafe { libc::atexit(at_exit) };
    for signal in [libc::SIGTERM, libc::SIGINT, libc::SIGHUP] {
        dispatch::on_signal(signal, quit);
    }
    autostart::refresh();
    displays::watch(menu::refresh_soon);

    let app = NSApplication::sharedApplication(mtm);
    app.setActivationPolicy(NSApplicationActivationPolicy::Accessory);
    dispatch::later(menu::start);
    app.run();
}

/// Only one ScreenYield at a time: a lock on a file in the settings folder, held until the end.
fn single_instance() -> bool {
    let dir = settings::dir();
    let _ = std::fs::create_dir_all(&dir);
    let file = std::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(false)
        .open(dir.join("lock"));
    let Ok(file) = file else {
        // Without a lock file better run twice than not at all
        return true;
    };
    if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } != 0 {
        return false;
    }
    std::mem::forget(file);
    true
}

/// Switches on every monitor ScreenYield switched off (listed in `off.txt`).
pub fn restore_all() {
    let off = settings::load_off();
    if off.is_empty() {
        return;
    }
    log!("switching on again: {off:?}");
    switch_on(&off);
}

/// Switches monitors on and forgets them; one that cannot come back (unplugged meanwhile) is logged.
pub fn switch_on(ids: &[u32]) {
    if let Err(e) = displays::set_enabled(ids, true) {
        log!("monitors {ids:?} not switched on together (error {e}), trying one by one");
        for &id in ids {
            if let Err(e) = displays::set_enabled(&[id], true) {
                log!("monitor {id} not switched on (error {e}), forgetting it");
            }
        }
    }
    let mut off = settings::load_off();
    off.retain(|id| !ids.contains(id));
    settings::save_off(&off);
}

/// Switches monitors off; they are written down first, so nothing is lost if the app ends now.
pub fn switch_off(ids: &[u32]) {
    let mut off = settings::load_off();
    let new: Vec<u32> = ids.iter().copied().filter(|id| !off.contains(id)).collect();
    off.extend(new);
    settings::save_off(&off);
    if let Err(e) = displays::set_enabled(ids, false) {
        log!("monitors {ids:?} not switched off (error {e})");
        off.retain(|id| !ids.contains(id));
        settings::save_off(&off);
    } else {
        log!("switched off: {ids:?}");
    }
}

pub fn quit() {
    log!("ScreenYield quits");
    restore_all();
    std::process::exit(0);
}

extern "C" fn at_exit() {
    restore_all();
}
