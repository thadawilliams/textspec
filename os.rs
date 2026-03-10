use sysinfo::System;
use crate::models::OsInfo;

pub fn collect(sys: &System) -> Option<OsInfo> {
    let name = System::long_os_version()?;
    let version = System::os_version().unwrap_or_default();
    let build = get_build_number();

    Some(OsInfo { name, version, build })
}

#[cfg(target_os = "windows")]
fn get_build_number() -> Option<String> {
    // Registry: HKLM\SOFTWARE\Microsoft\Windows NT\CurrentVersion → CurrentBuild
    use std::process::Command;
    let output = Command::new("reg")
        .args(["query", r"HKLM\SOFTWARE\Microsoft\Windows NT\CurrentVersion", "/v", "CurrentBuild"])
        .output()
        .ok()?;
    let text = String::from_utf8(output.stdout).ok()?;
    text.lines()
        .find(|l| l.contains("CurrentBuild"))
        .and_then(|l| l.split_whitespace().last().map(|s| s.to_string()))
}

#[cfg(target_os = "linux")]
fn get_build_number() -> Option<String> {
    use std::process::Command;
    let output = Command::new("uname").arg("-r").output().ok()?;
    String::from_utf8(output.stdout).ok().map(|s| s.trim().to_string())
}

#[cfg(target_os = "macos")]
fn get_build_number() -> Option<String> {
    use std::process::Command;
    let output = Command::new("sw_vers").arg("-buildVersion").output().ok()?;
    String::from_utf8(output.stdout).ok().map(|s| s.trim().to_string())
}
