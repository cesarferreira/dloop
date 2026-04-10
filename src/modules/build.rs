//! Gradle / gradlew subprocess runner.
use anyhow::{anyhow, Result};
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::Sender;
use std::thread;

pub fn find_gradlew(project_root: &Path) -> Option<PathBuf> {
    let unix = project_root.join("gradlew");
    if unix.is_file() {
        return Some(unix);
    }
    let win = project_root.join("gradlew.bat");
    if win.is_file() {
        return Some(win);
    }
    None
}

pub struct GradleSpawn {
    pub child: Child,
}

pub fn spawn_gradle(
    gradlew: &Path,
    project_root: &Path,
    task: &str,
    android_serial: Option<&str>,
    tx: Sender<String>,
) -> Result<GradleSpawn> {
    let mut cmd = Command::new(gradlew);
    cmd.current_dir(project_root);
    cmd.arg(task);
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());
    if let Some(s) = android_serial {
        cmd.env("ANDROID_SERIAL", s);
    }

    let mut child = cmd.spawn().map_err(|e| anyhow!("gradlew spawn failed: {e}"))?;

    let stdout = child.stdout.take().expect("stdout");
    let stderr = child.stderr.take().expect("stderr");
    let tx_out = tx.clone();
    thread::spawn(move || {
        let mut reader = BufReader::new(stdout);
        let mut line = String::new();
        loop {
            line.clear();
            match reader.read_line(&mut line) {
                Ok(0) => break,
                Ok(_) => {
                    let t = line.trim_end().to_string();
                    if tx_out.send(format!("[stdout] {t}")).is_err() {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
    });
    thread::spawn(move || {
        let mut reader = BufReader::new(stderr);
        let mut line = String::new();
        loop {
            line.clear();
            match reader.read_line(&mut line) {
                Ok(0) => break,
                Ok(_) => {
                    let t = line.trim_end().to_string();
                    if tx.send(format!("[stderr] {t}")).is_err() {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
    });

    Ok(GradleSpawn { child })
}
