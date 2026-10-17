//! Installing a new version from the menu, so no one needs a terminal for updates.
//!
//! The zip of the release is fetched with `curl` (no quarantine flag, so no Gatekeeper prompt),
//! checked to be signed by the same certificate as this app and put in place of this app; a small
//! shell waits for this process to end and opens the new one. If the folder is not writable, macOS
//! asks for an administrator password.

use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Progress {
    Downloading,
    Installing,
    /// Done; the app quits now and the new one starts.
    Restart,
    Failed(String),
}

/// `…/ScreenYield.app` this program runs from; without it, no update from the menu.
pub fn bundle() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let bundle = exe.ancestors().nth(3)?;
    (bundle.extension()? == "app").then(|| bundle.to_path_buf())
}

/// Installs the zip at `url` in a background thread; `report` is called there.
pub fn start(url: String, report: impl Fn(Progress) + Send + 'static) {
    let spawned = std::thread::Builder::new().name("self-update".into()).spawn(move || {
        let result = bundle()
            .ok_or_else(|| "not running from an app bundle".to_string())
            .and_then(|bundle| install_app(&url, &bundle, &report).and_then(|()| relaunch(&bundle)));
        match result {
            Ok(()) => report(Progress::Restart),
            Err(e) => {
                crate::log!("update failed: {e}");
                report(Progress::Failed(e));
            }
        }
    });
    if let Err(e) = spawned {
        crate::log!("update not started: {e}");
    }
}

fn checked(cmd: &mut Command, what: &str) -> Result<String, String> {
    let out = cmd.output().map_err(|e| format!("{what}: {e}"))?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        return Err(format!("{what}: {}", err.trim()));
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

fn work_dir() -> Result<PathBuf, String> {
    let dir = std::env::temp_dir().join(format!("screenyield-update-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir)
}

/// Puts the app from the zip at `url` in place of `bundle`.
fn install_app(url: &str, bundle: &Path, report: &impl Fn(Progress)) -> Result<(), String> {
    let dir = work_dir()?;
    report(Progress::Downloading);
    let zip = dir.join("update.zip");
    checked(
        Command::new("curl")
            .args(["-fsSL", "--max-time", "600", "-A", "ScreenYield-update", "-o"])
            .arg(&zip)
            .arg(url),
        "download",
    )?;
    report(Progress::Installing);
    let unpacked = dir.join("unpacked");
    checked(
        Command::new("ditto").args(["-x", "-k"]).arg(&zip).arg(&unpacked),
        "unpack",
    )?;
    let new = unpacked.join("ScreenYield.app");
    if !new.is_dir() {
        return Err("the download contains no ScreenYield.app".into());
    }
    // Only accept what the same certificate signed (not checkable for an ad-hoc signed build)
    let mut verify = Command::new("codesign");
    verify.args(["--verify", "--deep", "--strict"]);
    if let Some(requirement) = signer_requirement(bundle) {
        verify.arg(format!("-R={requirement}"));
    }
    checked(verify.arg(&new), "signature of the new version")?;
    replace(bundle, &new)?;
    let _ = std::fs::remove_dir_all(&dir);
    Ok(())
}

/// This app's designated requirement, if it names a certificate (self-signed: "certificate leaf").
fn signer_requirement(bundle: &Path) -> Option<String> {
    let out = Command::new("codesign").args(["-d", "-r-"]).arg(bundle).output().ok()?;
    let text = String::from_utf8_lossy(&out.stdout).into_owned() + &String::from_utf8_lossy(&out.stderr);
    let line = text.lines().find_map(|l| l.strip_prefix("designated => "))?;
    line.contains("certificate").then(|| line.trim().to_string())
}

fn replace(bundle: &Path, new: &Path) -> Result<(), String> {
    let parent = bundle.parent().ok_or("no folder")?;
    let old = parent.join(".ScreenYield-old.app");
    let _ = std::fs::remove_dir_all(&old);
    if std::fs::rename(bundle, &old).is_ok() {
        let copied = checked(Command::new("ditto").arg(new).arg(bundle), "install");
        if let Err(e) = copied {
            let _ = std::fs::remove_dir_all(bundle);
            let _ = std::fs::rename(&old, bundle);
            return Err(e);
        }
        let _ = std::fs::remove_dir_all(&old);
        return Ok(());
    }
    // Not writable for this user: macOS asks for an administrator password
    let quote = |p: &Path| format!("'{}'", p.display().to_string().replace('\'', r"'\''"));
    let script = format!(
        "rm -rf {old} && mv {bundle} {old} && ditto {new} {bundle} && rm -rf {old}",
        old = quote(&old),
        bundle = quote(bundle),
        new = quote(new),
    );
    let apple = format!(
        "do shell script \"{}\" with administrator privileges",
        script.replace('\\', "\\\\").replace('"', "\\\"")
    );
    checked(Command::new("osascript").args(["-e", &apple]), "install")
        .map(|_| ())
        .map_err(|e| if e.contains("-128") { "cancelled".into() } else { e })
}

/// Opens the new app as soon as this process has ended.
fn relaunch(bundle: &Path) -> Result<(), String> {
    Command::new("/bin/sh")
        .args([
            "-c",
            "while kill -0 \"$1\" 2>/dev/null; do sleep 0.2; done; open \"$0\"",
        ])
        .arg(bundle)
        .arg(std::process::id().to_string())
        .spawn()
        .map(|_| ())
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn zip(app: &Path, to: &Path) -> String {
        checked(
            Command::new("ditto")
                .args(["-c", "-k", "--keepParent"])
                .arg(app)
                .arg(to),
            "zip",
        )
        .unwrap();
        format!("file://{}", to.display())
    }

    /// Replaces a signed app with another build; needs one in `SY_TEST_APP` (scripts/bundle-macos.sh
    /// with the project certificate).
    #[test]
    #[ignore = "needs a bundled app"]
    fn replaces_a_signed_app() {
        let source = PathBuf::from(std::env::var("SY_TEST_APP").expect("SY_TEST_APP"));
        let dir = std::env::temp_dir().join(format!("screenyield-update-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("Applications")).unwrap();
        let installed = dir.join("Applications/ScreenYield.app");
        checked(Command::new("ditto").arg(&source).arg(&installed), "copy").unwrap();
        assert!(
            signer_requirement(&installed).is_some(),
            "the test app is signed ad-hoc"
        );
        std::fs::write(installed.join("Contents/old-marker"), "old").unwrap();
        let url = zip(&source, &dir.join("release.zip"));
        install_app(&url, &installed, &|_| {}).unwrap();
        assert!(installed.join("Contents/MacOS/ScreenYield").exists());
        assert!(!installed.join("Contents/old-marker").exists());
        assert!(!dir.join("Applications/.ScreenYield-old.app").exists());

        // Signed by someone else (here: ad-hoc): refused, the installed app stays
        let foreign = dir.join("foreign/ScreenYield.app");
        checked(Command::new("ditto").arg(&source).arg(&foreign), "copy").unwrap();
        checked(
            Command::new("codesign")
                .args(["--force", "--deep", "--sign", "-"])
                .arg(&foreign),
            "sign",
        )
        .unwrap();
        let url = zip(&foreign, &dir.join("foreign.zip"));
        let err = install_app(&url, &installed, &|_| {}).unwrap_err();
        assert!(err.contains("signature"), "{err}");
        assert!(installed.join("Contents/MacOS/ScreenYield").exists());
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
