//! Looking for a newer ScreenYield on GitHub.
//!
//! The only thing sent is the request for the newest release of the project. It goes through `curl`
//! (part of macOS), so the app needs no web client of its own. Errors (offline, no release) stay
//! silent.
//!
//! Every push to main rebuilds the test build without changing the version number. So for test
//! builds a release is also new if it was built from another, younger commit than this program.

use std::process::Command;

const LATEST_RELEASE_URL: &str = "https://api.github.com/repos/eckonator/ScreenYield/releases/latest";
/// GitHub does not list releases by date (the test build often comes last), so the newest is picked
/// from the first page by its publishing time.
const NEWEST_URL: &str = "https://api.github.com/repos/eckonator/ScreenYield/releases?per_page=20";
/// The file the app installs itself.
pub const ASSET_SUFFIX: &str = "-macOS.zip";

/// Built from a release tag (not a test build from main).
pub fn is_release() -> bool {
    env!("SY_CHANNEL") == "release"
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewVersion {
    pub version: String,
    /// Release page.
    pub url: String,
    /// The zip to install, if the release has one.
    pub asset: Option<String>,
}

/// `Ok(None)`: nothing new, `Err`: GitHub not reachable. Blocks for up to a few seconds.
pub fn check() -> Result<Option<NewVersion>, ()> {
    // SY_UPDATE_URL (debug builds only): another source, e.g. a local JSON file for testing
    let url = std::env::var("SY_UPDATE_URL")
        .ok()
        .filter(|_| cfg!(debug_assertions))
        .unwrap_or_else(|| if is_release() { LATEST_RELEASE_URL } else { NEWEST_URL }.into());
    let out = Command::new("curl")
        .args([
            "-fsSL",
            "--max-time",
            "8",
            "-H",
            "Accept: application/vnd.github+json",
            "-A",
            "ScreenYield-update-check",
        ])
        .arg(&url)
        .output()
        .ok()
        .filter(|o| o.status.success())
        .ok_or(())?;
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).map_err(|_| ())?;
    let release = json.as_array().and_then(|a| newest(a)).unwrap_or(&json);
    Ok(newer_release(
        release,
        env!("CARGO_PKG_VERSION"),
        // A release is new only with a higher version
        if is_release() { "" } else { env!("SY_GIT_HASH") },
        env!("SY_GIT_TIME"),
    ))
}

/// The release published last ("2026-…" times sort as text).
fn newest(releases: &[serde_json::Value]) -> Option<&serde_json::Value> {
    releases
        .iter()
        .max_by_key(|r| r.get("published_at").and_then(|p| p.as_str()).unwrap_or_default())
}

/// Compares a release (GitHub API object) with this build.
fn newer_release(release: &serde_json::Value, version: &str, commit: &str, commit_time: &str) -> Option<NewVersion> {
    let tag = release.get("tag_name")?.as_str()?;
    let page = release.get("html_url")?.as_str()?.to_string();
    let latest = tag.trim_start_matches('v');
    let asset = release
        .get("assets")
        .and_then(|a| a.as_array())
        .and_then(|assets| {
            assets
                .iter()
                .filter_map(|a| a.get("browser_download_url")?.as_str())
                .find(|u| u.ends_with(ASSET_SUFFIX))
        })
        .map(str::to_string);
    let found = NewVersion {
        version: latest.to_string(),
        url: page,
        asset,
    };
    if newer(latest, version) {
        return Some(found);
    }
    if latest != version || commit.is_empty() {
        return None;
    }
    // Same version: newer if built from another commit published after ours
    let release_commit = release.get("target_commitish")?.as_str()?;
    let published = release.get("published_at")?.as_str().and_then(unix_time)?;
    let ours: i64 = commit_time.parse().ok()?;
    (!release_commit.starts_with(commit) && published > ours).then_some(found)
}

/// Is `a` newer than `b`? Compares "1.2.3" part by part as numbers.
pub fn newer(a: &str, b: &str) -> bool {
    let parse = |v: &str| -> Vec<u64> {
        v.split(['.', '-', '+'])
            .take(3)
            .map(|p| p.parse().unwrap_or(0))
            .collect()
    };
    parse(a) > parse(b)
}

/// "2026-10-17T10:52:11Z" as seconds since 1970.
fn unix_time(text: &str) -> Option<i64> {
    let number = |range: std::ops::Range<usize>| text.get(range)?.parse::<i64>().ok();
    let (year, month, day) = (number(0..4)?, number(5..7)?, number(8..10)?);
    let (hour, minute, second) = (number(11..13)?, number(14..16)?, number(17..19)?);
    // Days since 1970 for a date in the Gregorian calendar
    let (y, m) = if month <= 2 {
        (year - 1, month + 9)
    } else {
        (year, month - 3)
    };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * m + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    Some(days * 86_400 + hour * 3600 + minute * 60 + second)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compares_versions() {
        assert!(newer("0.2.0", "0.1.9"));
        assert!(newer("0.10.0", "0.9.0"));
        assert!(!newer("0.1.2", "0.1.2"));
        assert!(!newer("0.1.0", "0.2.0"));
    }

    #[test]
    fn reads_github_times() {
        assert_eq!(unix_time("1970-01-01T00:00:00Z"), Some(0));
        assert_eq!(unix_time("2026-10-17T10:52:11Z"), Some(1_792_234_331));
        assert_eq!(unix_time("nonsense"), None);
    }

    #[test]
    fn same_version_counts_when_built_later_from_another_commit() {
        let release = serde_json::json!({
            "tag_name": "v0.1.0",
            "html_url": "https://example/releases/v0.1.0",
            "target_commitish": "abcdef1234",
            "published_at": "2026-10-19T12:00:00Z",
        });
        let noon = unix_time("2026-10-19T12:00:00Z").unwrap();
        let earlier = (noon - 3600).to_string();
        let later = (noon + 3600).to_string();
        assert!(newer_release(&release, "0.1.0", "1111111", &earlier).is_some());
        assert!(newer_release(&release, "0.1.0", "abcdef1", &earlier).is_none());
        assert!(newer_release(&release, "0.1.0", "1111111", &later).is_none());
        assert!(newer_release(&release, "0.1.0", "", "").is_none());
        assert!(newer_release(&release, "0.0.9", "", "").is_some());
    }

    #[test]
    fn the_newest_is_found_wherever_it_is_listed() {
        let list = serde_json::json!([
            {"tag_name": "v0.1.1", "published_at": "2026-10-19T16:25:09Z"},
            {"tag_name": "testing", "published_at": "2026-10-19T17:44:53Z"},
            {"tag_name": "v0.1.0", "published_at": "2026-10-19T16:11:12Z"},
        ]);
        assert_eq!(newest(list.as_array().unwrap()).unwrap()["tag_name"], "testing");
    }

    #[test]
    fn picks_the_zip() {
        let release = serde_json::json!({
            "tag_name": "v0.2.0",
            "html_url": "https://example/releases/v0.2.0",
            "assets": [
                {"browser_download_url": "https://example/ScreenYield-0.2.0-macOS.dmg"},
                {"browser_download_url": "https://example/ScreenYield-0.2.0-macOS.zip"},
            ],
        });
        let found = newer_release(&release, "0.1.0", "", "").unwrap();
        assert_eq!(
            found.asset.as_deref(),
            Some("https://example/ScreenYield-0.2.0-macOS.zip")
        );
    }
}
