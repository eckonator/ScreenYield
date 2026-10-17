//! A small log in `~/Library/Logs/ScreenYield/screenyield.log` (also on stderr), rotated at 1 MB.

use std::io::Write;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

const MAX_SIZE: u64 = 1024 * 1024;
const KEEP: usize = 2;

static FILE: Mutex<Option<std::fs::File>> = Mutex::new(None);

pub fn dir() -> Option<PathBuf> {
    std::env::var_os("HOME").map(|h| PathBuf::from(h).join("Library/Logs/ScreenYield"))
}

pub fn init() {
    let Some(dir) = dir() else { return };
    let _ = std::fs::create_dir_all(&dir);
    let path = dir.join("screenyield.log");
    if std::fs::metadata(&path).is_ok_and(|m| m.len() > MAX_SIZE) {
        for i in (1..KEEP).rev() {
            let _ = std::fs::rename(
                dir.join(format!("screenyield.log.{i}")),
                dir.join(format!("screenyield.log.{}", i + 1)),
            );
        }
        let _ = std::fs::rename(&path, dir.join("screenyield.log.1"));
    }
    if let Ok(file) = std::fs::OpenOptions::new().create(true).append(true).open(&path) {
        *FILE.lock().expect("log lock") = Some(file);
    }
}

pub fn write(text: &str) {
    let secs = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs());
    let line = format!("{} {text}\n", timestamp(secs));
    eprint!("{line}");
    if let Some(file) = FILE.lock().expect("log lock").as_mut() {
        let _ = file.write_all(line.as_bytes());
    }
}

/// "2026-10-19 07:43:32" (UTC) from seconds since 1970.
fn timestamp(secs: u64) -> String {
    let days = (secs / 86_400) as i64;
    let rest = secs % 86_400;
    // Civil date from days since 1970 (Gregorian calendar)
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!(
        "{year:04}-{month:02}-{day:02} {:02}:{:02}:{:02}",
        rest / 3600,
        rest / 60 % 60,
        rest % 60
    )
}

#[macro_export]
macro_rules! log {
    ($($arg:tt)*) => { $crate::log::write(&format!($($arg)*)) };
}

#[cfg(test)]
mod tests {
    #[test]
    fn timestamps_are_readable() {
        assert_eq!(super::timestamp(0), "1970-01-01 00:00:00");
        assert_eq!(super::timestamp(1_792_395_812), "2026-10-19 07:43:32");
    }
}
