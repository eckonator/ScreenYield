//! "Start at login" through a LaunchAgent in the user's library.

use std::path::PathBuf;

const LABEL: &str = "de.eckonator.screenyield";

fn plist() -> Option<PathBuf> {
    let home = std::env::var_os("HOME")?;
    Some(PathBuf::from(home).join(format!("Library/LaunchAgents/{LABEL}.plist")))
}

pub fn enabled() -> bool {
    plist().is_some_and(|p| p.exists())
}

pub fn set(on: bool) -> Result<(), String> {
    let path = plist().ok_or("no home folder")?;
    if !on {
        return match std::fs::remove_file(&path) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e.to_string()),
            _ => Ok(()),
        };
    }
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let exe = exe.display().to_string().replace('&', "&amp;").replace('<', "&lt;");
    let text = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>{LABEL}</string>
    <key>ProgramArguments</key>
    <array>
        <string>{exe}</string>
    </array>
    <key>RunAtLoad</key>
    <true/>
</dict>
</plist>
"#
    );
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    std::fs::write(&path, text).map_err(|e| e.to_string())
}

/// After an update or a move the entry must point at the program that runs now.
pub fn refresh() {
    if enabled()
        && let Err(e) = set(true)
    {
        crate::log!("start at login not updated: {e}");
    }
}
