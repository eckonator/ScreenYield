//! The commit the program is built from and whether it is a release (for the update check).

fn git(args: &[&str]) -> String {
    std::process::Command::new("git")
        .args(args)
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_default()
}

fn main() {
    println!("cargo:rustc-env=SY_GIT_HASH={}", git(&["rev-parse", "HEAD"]));
    println!(
        "cargo:rustc-env=SY_GIT_TIME={}",
        git(&["log", "-1", "--format=%ct", "HEAD"])
    );
    // SY_CHANNEL=release for builds of a release tag (set by the release workflow), else test build
    let channel = std::env::var("SY_CHANNEL").unwrap_or_default();
    let channel = if channel == "release" { "release" } else { "test" };
    println!("cargo:rustc-env=SY_CHANNEL={channel}");
    println!("cargo:rerun-if-env-changed=SY_CHANNEL");
    println!("cargo:rerun-if-changed=.git/HEAD");
    println!("cargo:rerun-if-changed=.git/refs/heads");
}
