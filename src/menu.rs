//! The symbol in the menu bar and its menu. The menu is built anew after every change; a click
//! is handled on the main thread after the menu has closed.

use std::cell::{Cell, RefCell};
use std::time::Duration;

use tray_icon::menu::{CheckMenuItem, Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use tray_icon::{Icon, TrayIcon, TrayIconBuilder};

use crate::i18n::tr;
use crate::model::Model;
use crate::{dispatch, displays, log, settings};

struct App {
    tray: TrayIcon,
    model: Model,
}

thread_local! {
    static APP: RefCell<Option<App>> = const { RefCell::new(None) };
    static REFRESH_PENDING: Cell<bool> = const { Cell::new(false) };
}

const MONITOR: &str = "monitor:";
const ALL_OFF: &str = "all-off";
const ALL_ON: &str = "all-on";
const AUTOSTART: &str = "autostart";
const QUIT: &str = "quit";

/// Puts the symbol into the menu bar (on the main thread, once the app runs).
pub fn start() {
    let mut model = Model::default();
    model.update(&displays::active(), &settings::load_off());
    let size = 36;
    let icon = match Icon::from_rgba(crate::icon::menu_bar(size), size, size) {
        Ok(icon) => icon,
        Err(e) => {
            log!("no menu bar symbol: {e}");
            std::process::exit(1);
        }
    };
    let tray = TrayIconBuilder::new()
        .with_icon_templated(icon)
        .with_tooltip("ScreenYield")
        .with_menu(Box::new(build(&model)))
        .build();
    let tray = match tray {
        Ok(tray) => tray,
        Err(e) => {
            log!("no menu bar symbol: {e}");
            std::process::exit(1);
        }
    };
    log_monitors(&model);
    APP.set(Some(App { tray, model }));
    MenuEvent::set_event_handler(Some(|event: MenuEvent| {
        let id = event.id.0;
        dispatch::later(move || clicked(&id));
    }));
}

/// After displays came, went or moved. macOS reports a change in several steps and the monitors
/// may still settle a moment later, so the menu is built now and once more shortly after.
pub fn refresh_soon() {
    if REFRESH_PENDING.replace(true) {
        return;
    }
    dispatch::later(|| {
        REFRESH_PENDING.set(false);
        refresh();
    });
    dispatch::after(Duration::from_millis(1500), refresh);
}

fn refresh() {
    APP.with_borrow_mut(|app| {
        let Some(app) = app else { return };
        let before = app.model.monitors.clone();
        app.model.update(&displays::active(), &settings::load_off());
        if app.model.monitors != before {
            log_monitors(&app.model);
        }
        app.tray.set_menu(Some(Box::new(build(&app.model))));
    });
}

fn log_monitors(model: &Model) {
    let list: Vec<String> = model
        .monitors
        .iter()
        .map(|m| {
            format!(
                "{}{}{}",
                m.id,
                if m.on { "" } else { " off" },
                if m.main { " main" } else { "" }
            )
        })
        .collect();
    log!("external monitors from left to right: [{}]", list.join(", "));
}

fn build(model: &Model) -> Menu {
    let menu = Menu::new();
    let separator = PredefinedMenuItem::separator;
    if !displays::supported() {
        let _ = menu.append(&MenuItem::new(tr("This macOS version is not supported"), false, None));
    } else if model.monitors.is_empty() {
        let _ = menu.append(&MenuItem::new(tr("No external monitor"), false, None));
    } else {
        for (i, m) in model.monitors.iter().enumerate() {
            let mut text = format!("{} {}", tr("Monitor"), i + 1);
            if m.main {
                text = format!("{text} – {}", tr("main display"));
            }
            let enabled = !m.on || model.may_switch_off(&[m.id]);
            let item = CheckMenuItem::with_id(format!("{MONITOR}{}", m.id), text, enabled, m.on, None);
            let _ = menu.append(&item);
        }
        let _ = menu.append(&separator());
        let on = model.on_ids();
        let all_off = MenuItem::with_id(
            ALL_OFF,
            tr("Switch all external monitors off"),
            model.may_switch_off(&on),
            None,
        );
        let all_on = MenuItem::with_id(
            ALL_ON,
            tr("Switch all external monitors on"),
            !model.off_ids().is_empty(),
            None,
        );
        let _ = menu.append_items(&[&all_off, &all_on]);
    }
    let _ = menu.append(&separator());
    let autostart = CheckMenuItem::with_id(AUTOSTART, tr("Start at login"), true, crate::autostart::enabled(), None);
    let _ = menu.append(&autostart);
    let _ = menu.append(&separator());
    let version = MenuItem::new(format!("ScreenYield {}", crate::version()), false, None);
    let quit = MenuItem::with_id(QUIT, tr("Quit ScreenYield"), true, None);
    let _ = menu.append_items(&[&version, &quit]);
    menu
}

fn clicked(id: &str) {
    // The model as the menu showed it; nothing stays borrowed while displays change
    let Some((on, off, monitor)) = APP.with_borrow(|app| {
        let model = &app.as_ref()?.model;
        let monitor = id
            .strip_prefix(MONITOR)
            .and_then(|n| n.parse::<u32>().ok())
            .and_then(|n| model.monitors.iter().find(|m| m.id == n))
            .map(|m| (m.id, m.on, model.may_switch_off(&[m.id])));
        let all_on = model.on_ids();
        Some((
            model.may_switch_off(&all_on).then_some(all_on),
            model.off_ids(),
            monitor,
        ))
    }) else {
        return;
    };
    match id {
        ALL_OFF => {
            if let Some(ids) = on {
                crate::switch_off(&ids);
            }
        }
        ALL_ON => crate::switch_on(&off),
        AUTOSTART => {
            let enable = !crate::autostart::enabled();
            if let Err(e) = crate::autostart::set(enable) {
                log!("start at login not changed: {e}");
            }
        }
        QUIT => crate::quit(),
        _ => match monitor {
            Some((id, true, true)) => crate::switch_off(&[id]),
            Some((id, false, _)) => crate::switch_on(&[id]),
            _ => {}
        },
    }
    refresh_soon();
}
