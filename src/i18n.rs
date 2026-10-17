//! Menu texts in English (the keys) and German, chosen by the first preferred system language.

use std::sync::OnceLock;

use core_foundation_sys::array::{CFArrayGetCount, CFArrayGetValueAtIndex};
use core_foundation_sys::base::CFRelease;
use core_foundation_sys::locale::CFLocaleCopyPreferredLanguages;
use core_foundation_sys::string::{CFStringGetCString, CFStringRef, kCFStringEncodingUTF8};

fn german() -> bool {
    static GERMAN: OnceLock<bool> = OnceLock::new();
    *GERMAN.get_or_init(|| {
        if let Ok(lang) = std::env::var("SY_LANG") {
            return lang.starts_with("de");
        }
        first_language().is_some_and(|l| l.starts_with("de"))
    })
}

fn first_language() -> Option<String> {
    unsafe {
        let languages = CFLocaleCopyPreferredLanguages();
        if languages.is_null() {
            return None;
        }
        let mut text = None;
        if CFArrayGetCount(languages) > 0 {
            let first = CFArrayGetValueAtIndex(languages, 0) as CFStringRef;
            let mut buf = [0i8; 32];
            if CFStringGetCString(first, buf.as_mut_ptr(), buf.len() as _, kCFStringEncodingUTF8) != 0 {
                text = Some(std::ffi::CStr::from_ptr(buf.as_ptr()).to_string_lossy().into_owned());
            }
        }
        CFRelease(languages.cast());
        text
    }
}

pub fn tr(key: &'static str) -> &'static str {
    if !german() {
        return key;
    }
    GERMAN.iter().find(|(en, _)| *en == key).map_or(key, |(_, de)| de)
}

const GERMAN: &[(&str, &str)] = &[
    ("Monitor", "Monitor"),
    ("main display", "Hauptbildschirm"),
    ("No external monitor", "Kein externer Monitor"),
    ("Switch all external monitors off", "Alle externen Monitore ausschalten"),
    ("Switch all external monitors on", "Alle externen Monitore einschalten"),
    ("Start at login", "Beim Anmelden starten"),
    ("Quit ScreenYield", "ScreenYield beenden"),
    (
        "This macOS version is not supported",
        "Diese macOS-Version wird nicht unterstützt",
    ),
];

#[cfg(test)]
mod tests {
    use super::GERMAN;

    /// Every text passed to `tr` in the sources has a German text (also calls over several lines).
    #[test]
    fn every_text_is_translated() {
        let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/src");
        let mut missing = Vec::new();
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            let source = std::fs::read_to_string(&path).unwrap();
            let compact: String = source.split_whitespace().collect::<Vec<_>>().join(" ");
            for part in compact.split("tr(\"").skip(1) {
                let key = &part[..part.find('"').unwrap()];
                if !GERMAN.iter().any(|(en, _)| *en == key) {
                    missing.push(key.to_string());
                }
            }
        }
        assert!(missing.is_empty(), "missing German texts: {missing:?}");
    }
}
