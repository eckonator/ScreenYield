//! The symbol in the menu bar and its menu, and when to look for updates. The menu is built anew
//! after every change; a click is handled on the main thread after the menu has closed.

use std::cell::{Cell, RefCell};
use std::time::Duration;

use tray_icon::menu::{CheckMenuItem, Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use tray_icon::{Icon, TrayIcon, TrayIconBuilder};

use crate::i18n::tr;
use crate::model::Model;
use crate::self_update::Progress;
use crate::settings::Settings;
use crate::update::NewVersion;
use crate::{dispatch, displays, log, self_update, settings, update};

/// First look for an update this long after the start, then every `CHECK_EVERY`.
const FIRST_CHECK: Duration = Duration::from_secs(10);
const CHECK_EVERY: Duration = Duration::from_secs(6 * 3600);
const ICON_SIZE: u32 = 36;

#[derive(Debug, Clone, PartialEq)]
enum UpdateState {
    Idle,
    /// Asked for in the menu: the answer is shown, also when there is nothing new.
    Checking,
    UpToDate,
    Unreachable,
    Available(NewVersion),
    Busy(Progress),
    Failed(NewVersion, String),
}

impl UpdateState {
    fn version(&self) -> Option<&NewVersion> {
        match self {
            Self::Available(v) | Self::Failed(v, _) => Some(v),
            _ => None,
        }
    }
}

struct App {
    tray: TrayIcon,
    model: Model,
    settings: Settings,
    update: UpdateState,
    /// The menu bar symbol shows the update dot.
    dot: bool,
}

thread_local! {
    static APP: RefCell<Option<App>> = const { RefCell::new(None) };
    static REFRESH_PENDING: Cell<bool> = const { Cell::new(false) };
}

const MONITOR: &str = "monitor:";
const ALL_OFF: &str = "all-off";
const ALL_ON: &str = "all-on";
const AUTOSTART: &str = "autostart";
const UPDATE: &str = "update";
const CHECK_AUTOMATICALLY: &str = "check-automatically";
const CHECK_NOW: &str = "check-now";
const QUIT: &str = "quit";

/// Puts the symbol into the menu bar (on the main thread, once the app runs).
pub fn start() {
    let mut model = Model::default();
    model.update(&displays::active(), &settings::load_off());
    let tray = icon(false).and_then(|icon| {
        TrayIconBuilder::new()
            .with_icon_templated(icon)
            .with_tooltip("ScreenYield")
            .build()
            .map_err(|e| e.to_string())
    });
    let tray = match tray {
        Ok(tray) => tray,
        Err(e) => {
            log!("no menu bar symbol: {e}");
            std::process::exit(1);
        }
    };
    log_monitors(&model);
    let app = App {
        tray,
        model,
        settings: Settings::load(),
        update: UpdateState::Idle,
        dot: false,
    };
    app.tray.set_menu(Some(Box::new(build(&app))));
    APP.set(Some(app));
    MenuEvent::set_event_handler(Some(|event: MenuEvent| {
        let id = event.id.0;
        dispatch::later(move || clicked(&id));
    }));
    dispatch::after(FIRST_CHECK, check_automatically);
}

fn icon(dot: bool) -> Result<Icon, String> {
    Icon::from_rgba(crate::icon::menu_bar(ICON_SIZE, dot), ICON_SIZE, ICON_SIZE).map_err(|e| e.to_string())
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
        show(app);
    });
}

/// Puts the menu and the symbol in line with the state.
fn show(app: &mut App) {
    app.tray.set_menu(Some(Box::new(build(app))));
    let dot = app.update.version().is_some();
    if dot != app.dot
        && let Ok(icon) = icon(dot)
    {
        let _ = app.tray.set_icon_templated(Some(icon));
        app.dot = dot;
    }
}

fn with_app(f: impl FnOnce(&mut App)) {
    APP.with_borrow_mut(|app| {
        if let Some(app) = app {
            f(app);
            show(app);
        }
    });
}

fn check_automatically() {
    if APP.with_borrow(|app| app.as_ref().is_some_and(|a| a.settings.check_updates)) {
        check(false);
    }
    dispatch::after(CHECK_EVERY, check_automatically);
}

/// Asks GitHub in the background; `asked`: chosen in the menu, so the answer is shown.
fn check(asked: bool) {
    if asked {
        with_app(|app| app.update = UpdateState::Checking);
    }
    let spawned = std::thread::Builder::new().name("update-check".into()).spawn(move || {
        let result = update::check();
        dispatch::later(move || checked(result, asked));
    });
    if let Err(e) = spawned {
        log!("update check not started: {e}");
    }
}

fn checked(result: Result<Option<NewVersion>, ()>, asked: bool) {
    with_app(|app| {
        if matches!(app.update, UpdateState::Busy(_)) {
            return;
        }
        app.update = match result {
            Ok(Some(new)) => {
                if app.update.version() != Some(&new) {
                    log!("update available: {}", new.version);
                }
                UpdateState::Available(new)
            }
            Ok(None) if asked => UpdateState::UpToDate,
            Err(()) if asked => UpdateState::Unreachable,
            // A failed automatic check keeps what was found before
            Err(()) => app.update.clone(),
            Ok(None) => UpdateState::Idle,
        };
    });
}

fn install(new: NewVersion) {
    let Some(url) = new.asset.clone().filter(|_| self_update::bundle().is_some()) else {
        // Not installable from here (no app bundle or no zip): the release page instead
        let _ = std::process::Command::new("open").arg(&new.url).spawn();
        return;
    };
    log!("installing update {}", new.version);
    with_app(|app| app.update = UpdateState::Busy(Progress::Downloading));
    self_update::start(url, move |progress| {
        let new = new.clone();
        dispatch::later(move || progressed(progress, new));
    });
}

fn progressed(progress: Progress, new: NewVersion) {
    match progress {
        Progress::Restart => {
            log!("update {} installed, starting it", new.version);
            crate::quit();
        }
        Progress::Failed(e) => with_app(|app| app.update = UpdateState::Failed(new, e)),
        busy => with_app(|app| app.update = UpdateState::Busy(busy)),
    }
}

fn log_monitors(model: &Model) {
    let list: Vec<String> = model
        .monitors
        .iter()
        .map(|m| {
            format!(
                "{} \"{}\"{}{}",
                m.id,
                m.name,
                if m.on { "" } else { " off" },
                if m.main { " main" } else { "" }
            )
        })
        .collect();
    log!("external monitors from left to right: [{}]", list.join(", "));
}

fn build(app: &App) -> Menu {
    let model = &app.model;
    let menu = Menu::new();
    let separator = PredefinedMenuItem::separator;
    if !displays::supported() {
        let _ = menu.append(&MenuItem::new(tr("This macOS version is not supported"), false, None));
    } else if model.monitors.is_empty() {
        let _ = menu.append(&MenuItem::new(tr("No external monitor"), false, None));
    } else {
        for (i, m) in model.monitors.iter().enumerate() {
            let mut text = format!("{} {}", tr("Monitor"), i + 1);
            if !m.name.is_empty() {
                text = format!("{text} ({})", m.name);
            }
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
    let status = match &app.update {
        UpdateState::Idle | UpdateState::Available(_) => None,
        UpdateState::Checking => Some(tr("Checking for updates …").to_string()),
        UpdateState::UpToDate => Some(tr("ScreenYield is up to date").to_string()),
        UpdateState::Unreachable => Some(tr("GitHub not reachable").to_string()),
        UpdateState::Busy(Progress::Downloading) => Some(tr("Downloading update …").to_string()),
        UpdateState::Busy(_) => Some(tr("Installing update …").to_string()),
        UpdateState::Failed(_, e) => Some(format!("{} {e}", tr("Update failed:"))),
    };
    if let Some(status) = status {
        let _ = menu.append(&MenuItem::new(status, false, None));
    }
    if let Some(new) = app.update.version() {
        let text = tr("Install update to {}").replace("{}", &new.version);
        let _ = menu.append(&MenuItem::with_id(UPDATE, text, true, None));
    }
    let automatically = CheckMenuItem::with_id(
        CHECK_AUTOMATICALLY,
        tr("Check for updates automatically"),
        true,
        app.settings.check_updates,
        None,
    );
    let _ = menu.append(&automatically);
    if !app.settings.check_updates {
        let busy = matches!(app.update, UpdateState::Checking | UpdateState::Busy(_));
        let _ = menu.append(&MenuItem::with_id(CHECK_NOW, tr("Check for updates"), !busy, None));
    }
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
    let Some((on, off, monitor, new)) = APP.with_borrow(|app| {
        let app = app.as_ref()?;
        let model = &app.model;
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
            app.update.version().cloned(),
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
        UPDATE => {
            if let Some(new) = new {
                install(new);
            }
        }
        CHECK_AUTOMATICALLY => {
            let mut on = false;
            with_app(|app| {
                app.settings.check_updates = !app.settings.check_updates;
                app.settings.save();
                on = app.settings.check_updates;
            });
            if on {
                check(false);
            }
        }
        CHECK_NOW => check(true),
        QUIT => crate::quit(),
        _ => match monitor {
            Some((id, true, true)) => crate::switch_off(&[id]),
            Some((id, false, _)) => crate::switch_on(&[id]),
            _ => {}
        },
    }
    refresh_soon();
}
