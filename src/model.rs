//! Which external monitors there are, which of them ScreenYield switched off, and what may be
//! switched off. Without system calls, so the rules are tested.

use crate::displays::Display;

#[derive(Debug, Clone, PartialEq)]
pub struct Monitor {
    pub id: u32,
    /// Left edge (points); for a monitor that is off, where it was.
    pub x: f64,
    pub on: bool,
    pub main: bool,
    /// What the monitor calls itself, empty if unknown; kept while it is off.
    pub name: String,
}

#[derive(Debug, Default)]
pub struct Model {
    /// External monitors from left to right.
    pub monitors: Vec<Monitor>,
    /// The built-in screen has a picture (the lid is open).
    pub builtin_on: bool,
}

impl Model {
    /// Takes the displays macOS reports now; `off` are the monitors ScreenYield switched off
    /// (they are not reported while off).
    pub fn update(&mut self, active: &[Display], off: &[u32]) {
        let before = std::mem::take(&mut self.monitors);
        self.builtin_on = active.iter().any(|d| d.builtin);
        let old_name = |id: u32| {
            before
                .iter()
                .find(|m| m.id == id)
                .map(|m| m.name.clone())
                .unwrap_or_default()
        };
        for d in active.iter().filter(|d| !d.builtin) {
            self.monitors.push(Monitor {
                id: d.id,
                x: d.x,
                on: true,
                main: d.main,
                name: if d.name.is_empty() {
                    old_name(d.id)
                } else {
                    base_name(&d.name).to_string()
                },
            });
        }
        for id in off {
            if self.monitors.iter().any(|m| m.id == *id) {
                continue;
            }
            let x = before.iter().find(|m| m.id == *id).map_or(f64::MAX, |m| m.x);
            self.monitors.push(Monitor {
                id: *id,
                x,
                on: false,
                main: false,
                name: old_name(*id),
            });
        }
        self.monitors.sort_by(|a, b| a.x.total_cmp(&b.x).then(a.id.cmp(&b.id)));
        // While some are off, macOS moves the others around (even to the other side of the
        // built-in screen), so the numbers stay as they were; new monitors come after
        if !off.is_empty() {
            let place = |id: u32| before.iter().position(|m| m.id == id).unwrap_or(usize::MAX);
            self.monitors.sort_by_key(|m| place(m.id));
        }
    }

    /// Screens that keep a picture when `ids` go off.
    fn remaining(&self, ids: &[u32]) -> usize {
        let monitors = self.monitors.iter().filter(|m| m.on && !ids.contains(&m.id)).count();
        monitors + usize::from(self.builtin_on)
    }

    /// Switching `ids` off must leave at least one screen.
    pub fn may_switch_off(&self, ids: &[u32]) -> bool {
        !ids.is_empty() && self.remaining(ids) > 0
    }

    pub fn on_ids(&self) -> Vec<u32> {
        self.monitors.iter().filter(|m| m.on).map(|m| m.id).collect()
    }

    pub fn off_ids(&self) -> Vec<u32> {
        self.monitors.iter().filter(|m| !m.on).map(|m| m.id).collect()
    }
}

/// macOS numbers monitors with the same name ("LEN L28u-30 (1)", "… (2)") in an order of its own
/// and renumbers them when one goes off; the menu numbers them itself, so that part is left out.
fn base_name(name: &str) -> &str {
    let Some(start) = name.strip_suffix(')').and_then(|n| n.rfind(" (")) else {
        return name;
    };
    let number = &name[start + 2..name.len() - 1];
    if !number.is_empty() && number.bytes().all(|b| b.is_ascii_digit()) {
        &name[..start]
    } else {
        name
    }
}

/// The monitors switched off, kept in a file while they are off: macOS forgets them as long as
/// they are off, and after a crash the next start switches them on again.
pub fn encode(ids: &[u32]) -> String {
    ids.iter().map(|id| format!("{id}\n")).collect()
}

pub fn decode(text: &str) -> Vec<u32> {
    text.lines().filter_map(|l| l.trim().parse().ok()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn display(id: u32, x: f64, builtin: bool, main: bool) -> Display {
        let name = if builtin { "Built-in Display" } else { "LEN L28u-30" };
        Display {
            id,
            x,
            builtin,
            main,
            name: name.into(),
        }
    }

    /// Monitor 1 / Monitor 2 (main) / MacBook, as on the desk this was written for.
    fn desk() -> Vec<Display> {
        vec![
            display(2, 0.0, false, true),
            display(1, 2560.0, true, false),
            display(3, -2560.0, false, false),
        ]
    }

    #[test]
    fn monitors_are_numbered_from_left_to_right() {
        let mut model = Model::default();
        model.update(&desk(), &[]);
        let ids: Vec<u32> = model.monitors.iter().map(|m| m.id).collect();
        assert_eq!(ids, [3, 2]);
        assert!(model.builtin_on);
        assert!(model.monitors[1].main);
    }

    #[test]
    fn a_monitor_that_is_off_keeps_its_place() {
        let mut model = Model::default();
        model.update(&desk(), &[]);
        // Monitor 1 off: macOS reports only the other two (and moves them)
        model.update(&[display(1, 0.0, true, true), display(2, 1728.0, false, false)], &[3]);
        let order: Vec<(u32, bool)> = model.monitors.iter().map(|m| (m.id, m.on)).collect();
        assert_eq!(order, [(3, false), (2, true)]);
        assert_eq!(model.off_ids(), [3]);
        assert_eq!(model.monitors[0].name, "LEN L28u-30", "the name stays while off");
        assert_eq!(model.on_ids(), [2]);
    }

    #[test]
    fn numbers_stay_when_macos_moves_the_others() {
        let mut model = Model::default();
        model.update(&desk(), &[]);
        // Monitor 2 off: macOS puts monitor 1 to the right of the MacBook
        model.update(&[display(1, 0.0, true, true), display(3, 1728.0, false, false)], &[2]);
        let order: Vec<(u32, bool)> = model.monitors.iter().map(|m| (m.id, m.on)).collect();
        assert_eq!(order, [(3, true), (2, false)]);
        // Both on again: the real arrangement counts
        model.update(&desk(), &[]);
        let ids: Vec<u32> = model.monitors.iter().map(|m| m.id).collect();
        assert_eq!(ids, [3, 2]);
    }

    #[test]
    fn the_counter_of_macos_is_left_out_of_names() {
        assert_eq!(base_name("LEN L28u-30 (2)"), "LEN L28u-30");
        assert_eq!(base_name("LEN L28u-30"), "LEN L28u-30");
        assert_eq!(base_name("Studio Display (USB-C)"), "Studio Display (USB-C)");
        assert_eq!(base_name("()"), "()");
        let mut model = Model::default();
        let mut desk = desk();
        desk[0].name = "LEN L28u-30 (1)".into();
        desk[2].name = "LEN L28u-30 (2)".into();
        model.update(&desk, &[]);
        assert!(model.monitors.iter().all(|m| m.name == "LEN L28u-30"));
    }

    #[test]
    fn the_last_screen_stays_on() {
        let mut model = Model::default();
        model.update(&desk(), &[]);
        assert!(model.may_switch_off(&[3, 2]), "the MacBook screen remains");
        // Lid closed: only the two monitors
        model.update(&[display(2, 0.0, false, true), display(3, -2560.0, false, false)], &[]);
        assert!(model.may_switch_off(&[3]));
        assert!(!model.may_switch_off(&[3, 2]));
        assert!(!model.may_switch_off(&[]));
    }

    #[test]
    fn the_list_of_monitors_that_are_off_survives_a_round_trip() {
        assert_eq!(decode(&encode(&[3, 7])), [3, 7]);
        assert_eq!(decode("3\n\nnonsense\n 9 \n"), [3, 9]);
        assert!(decode("").is_empty());
    }
}
