//! `bd doctor` — verify external tools and Android project setup.
use anyhow::{anyhow, Result};
use crossterm::style::{style, Attribute, Color, Stylize};
use std::fmt;
use std::io::IsTerminal;
use std::path::Path;
use std::process::Command;

use crate::adb::AdbClient;
use crate::modules::build::find_gradlew;
use crate::modules::mirror::scrcpy_path;
use crate::modules::project::{find_app_gradle, infer_project};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CheckStatus {
    Ok,
    Warn,
    Fail,
}

impl fmt::Display for CheckStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Ok => write!(f, "ok"),
            Self::Warn => write!(f, "warn"),
            Self::Fail => write!(f, "fail"),
        }
    }
}

struct CheckResult {
    status: CheckStatus,
    label: &'static str,
    detail: String,
}

pub fn run_doctor(project_root: &Path) -> Result<()> {
    let color = std::io::stdout().is_terminal();
    let mut failures = 0usize;
    let mut warnings = 0usize;

    println!("{}", banner("ByeDroid doctor", color));
    println!(
        "{} {}",
        label("Project:", color),
        path_value(project_root.display().to_string(), color)
    );
    println!();

    for check in collect_checks(project_root) {
        match check.status {
            CheckStatus::Fail => failures += 1,
            CheckStatus::Warn => warnings += 1,
            CheckStatus::Ok => {}
        }
        println!(
            "{} {} {}",
            status_badge(check.status, color),
            check_label(check.label, color),
            check.detail
        );
    }

    println!();
    if failures == 0 {
        println!("{}", success_summary(warnings, color));
        Ok(())
    } else {
        println!("{}", failure_summary(failures, warnings, color));
        Err(anyhow!("doctor found {failures} blocking issue(s)"))
    }
}

fn collect_checks(project_root: &Path) -> Vec<CheckResult> {
    let mut checks = Vec::new();

    checks.push(CheckResult {
        status: CheckStatus::Ok,
        label: "project root",
        detail: project_root.display().to_string(),
    });

    let adb = match AdbClient::new() {
        Ok(adb) => {
            let version = adb
                .run_command(&["version"])
                .ok()
                .and_then(|out| {
                    let stdout = String::from_utf8_lossy(&out.stdout);
                    stdout
                        .lines()
                        .find(|line| !line.trim().is_empty())
                        .map(|line| line.trim().to_string())
                })
                .unwrap_or_else(|| adb.adb_path.display().to_string());
            checks.push(CheckResult {
                status: CheckStatus::Ok,
                label: "adb",
                detail: format!("{} ({version})", adb.adb_path.display()),
            });
            Some(adb)
        }
        Err(err) => {
            checks.push(CheckResult {
                status: CheckStatus::Fail,
                label: "adb",
                detail: err.to_string(),
            });
            None
        }
    };

    let java_path = which::which("java").ok();
    checks.push(match java_path {
        Some(path) => CheckResult {
            status: CheckStatus::Ok,
            label: "java",
            detail: format!(
                "{}{}",
                path.display(),
                java_version_suffix(path.as_path()).unwrap_or_default()
            ),
        },
        None => CheckResult {
            status: CheckStatus::Warn,
            label: "java",
            detail: "not found in PATH; Gradle builds may fail".to_string(),
        },
    });

    let gradlew = find_gradlew(project_root);
    checks.push(match gradlew {
        Some(ref path) => {
            let executable = is_executable(path);
            CheckResult {
                status: if executable {
                    CheckStatus::Ok
                } else {
                    CheckStatus::Warn
                },
                label: "gradlew",
                detail: if executable {
                    path.display().to_string()
                } else {
                    format!("{} (not executable)", path.display())
                },
            }
        }
        None => CheckResult {
            status: CheckStatus::Fail,
            label: "gradlew",
            detail: "not found in project root".to_string(),
        },
    });

    let app_gradle = find_app_gradle(project_root);
    checks.push(match app_gradle {
        Some(ref path) => CheckResult {
            status: CheckStatus::Ok,
            label: "android app module",
            detail: path.display().to_string(),
        },
        None => CheckResult {
            status: CheckStatus::Fail,
            label: "android app module",
            detail: "no application build.gradle(.kts) found".to_string(),
        },
    });

    checks.push(match infer_project(project_root) {
        Ok(inference) if inference.gradle_file.is_some() => {
            let mut parts = vec![format!("variant {}", inference.variant_summary)];
            if let Some(task) = inference.assemble_task {
                parts.push(format!("assemble {task}"));
            }
            if let Some(task) = inference.install_task {
                parts.push(format!("install {task}"));
            }
            if !inference.application_ids.is_empty() {
                parts.push(format!("packages {}", inference.application_ids.join(", ")));
            }
            CheckResult {
                status: CheckStatus::Ok,
                label: "project inference",
                detail: parts.join(" | "),
            }
        }
        Ok(_) => CheckResult {
            status: CheckStatus::Warn,
            label: "project inference",
            detail: "no Android app module inferred".to_string(),
        },
        Err(err) => CheckResult {
            status: CheckStatus::Fail,
            label: "project inference",
            detail: err.to_string(),
        },
    });

    checks.push(match sdk_env_detail() {
        Some(detail) => CheckResult {
            status: CheckStatus::Ok,
            label: "android sdk env",
            detail,
        },
        None => CheckResult {
            status: CheckStatus::Warn,
            label: "android sdk env",
            detail: "ANDROID_HOME and ANDROID_SDK_ROOT are both unset".to_string(),
        },
    });

    checks.push(match scrcpy_path() {
        Some(path) => CheckResult {
            status: CheckStatus::Ok,
            label: "scrcpy",
            detail: path.display().to_string(),
        },
        None => CheckResult {
            status: CheckStatus::Warn,
            label: "scrcpy",
            detail: "not found in PATH; screen mirror hotkey `m` will be unavailable".to_string(),
        },
    });

    checks.push(match adb {
        Some(adb) => match adb.list_devices() {
            Ok(devices) if devices.is_empty() => CheckResult {
                status: CheckStatus::Warn,
                label: "connected devices",
                detail: "no ADB devices connected".to_string(),
            },
            Ok(devices) => {
                let names = devices
                    .iter()
                    .map(|d| format!("{} ({})", d.model, d.serial))
                    .collect::<Vec<_>>()
                    .join(", ");
                CheckResult {
                    status: CheckStatus::Ok,
                    label: "connected devices",
                    detail: names,
                }
            }
            Err(err) => CheckResult {
                status: CheckStatus::Warn,
                label: "connected devices",
                detail: format!("adb listed devices unsuccessfully: {err}"),
            },
        },
        None => CheckResult {
            status: CheckStatus::Warn,
            label: "connected devices",
            detail: "skipped because adb is unavailable".to_string(),
        },
    });

    checks
}

fn java_version_suffix(path: &Path) -> Option<String> {
    let output = Command::new(path).arg("-version").output().ok()?;
    let text = if output.stderr.is_empty() {
        String::from_utf8_lossy(&output.stdout).into_owned()
    } else {
        String::from_utf8_lossy(&output.stderr).into_owned()
    };
    let line = text.lines().find(|line| !line.trim().is_empty())?;
    Some(format!(" ({})", line.trim()))
}

fn sdk_env_detail() -> Option<String> {
    for key in ["ANDROID_SDK_ROOT", "ANDROID_HOME"] {
        if let Ok(value) = std::env::var(key) {
            if !value.trim().is_empty() {
                return Some(format!("{key}={value}"));
            }
        }
    }
    None
}

fn banner(text: &str, color: bool) -> String {
    if color {
        format!(
            "{}",
            style(text)
                .with(Color::Cyan)
                .attribute(Attribute::Bold)
                .attribute(Attribute::Underlined)
        )
    } else {
        text.to_string()
    }
}

fn label(text: &str, color: bool) -> String {
    if color {
        format!(
            "{}",
            style(text).with(Color::DarkCyan).attribute(Attribute::Bold)
        )
    } else {
        text.to_string()
    }
}

fn path_value(text: String, color: bool) -> String {
    if color {
        format!("{}", style(text).with(Color::White))
    } else {
        text
    }
}

fn check_label(text: &str, color: bool) -> String {
    if color {
        format!("{}", style(format!("{text}:")).attribute(Attribute::Bold))
    } else {
        format!("{text}:")
    }
}

fn status_badge(status: CheckStatus, color: bool) -> String {
    let text = match status {
        CheckStatus::Ok => " OK ",
        CheckStatus::Warn => "WARN",
        CheckStatus::Fail => "FAIL",
    };
    if color {
        let styled = match status {
            CheckStatus::Ok => style(text).with(Color::Black).on(Color::Green),
            CheckStatus::Warn => style(text).with(Color::Black).on(Color::Yellow),
            CheckStatus::Fail => style(text).with(Color::White).on(Color::Red),
        };
        format!("{}", styled.attribute(Attribute::Bold))
    } else {
        format!("[{}]", status)
    }
}

fn success_summary(warnings: usize, color: bool) -> String {
    let base = if warnings == 0 {
        "Doctor passed with no warnings".to_string()
    } else {
        format!("Doctor passed with {warnings} warning(s)")
    };
    if color {
        format!(
            "{}",
            style(base).with(Color::Green).attribute(Attribute::Bold)
        )
    } else {
        base
    }
}

fn failure_summary(failures: usize, warnings: usize, color: bool) -> String {
    let base = if warnings == 0 {
        format!("Doctor found {failures} blocking issue(s)")
    } else {
        format!("Doctor found {failures} blocking issue(s) and {warnings} warning(s)")
    };
    if color {
        format!(
            "{}",
            style(base).with(Color::Red).attribute(Attribute::Bold)
        )
    } else {
        base
    }
}

#[cfg(unix)]
fn is_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    path.metadata()
        .map(|meta| meta.permissions().mode() & 0o111 != 0)
        .unwrap_or(false)
}

#[cfg(not(unix))]
fn is_executable(path: &Path) -> bool {
    path.is_file()
}
