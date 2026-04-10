//! ADB subprocess wrapper (adapted from dab's AdbClient).
use anyhow::{anyhow, Result};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use which::which;

#[derive(Debug, Clone)]
pub struct Device {
    pub serial: String,
    pub state: String,
    pub model: String,
}

pub struct AdbClient {
    pub adb_path: PathBuf,
}

impl AdbClient {
    pub fn new() -> Result<Self> {
        let adb_path = which("adb").map_err(|_| anyhow!("adb not found in PATH"))?;
        Ok(Self { adb_path })
    }

    pub fn run_command(&self, args: &[&str]) -> Result<std::process::Output> {
        let output = Command::new(&self.adb_path).args(args).output()?;
        Ok(output)
    }

    /// Parse `adb devices -l` into structured devices (model from key=value on line).
    pub fn list_devices(&self) -> Result<Vec<Device>> {
        let output = self.run_command(&["devices", "-l"])?;
        let stdout = String::from_utf8_lossy(&output.stdout);
        let mut devices = Vec::new();

        for line in stdout.lines() {
            if line.is_empty()
                || line.contains("List of devices attached")
                || line.contains("daemon not running")
                || line.contains("daemon started")
            {
                continue;
            }
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() < 2 {
                continue;
            }
            let serial = parts[0].to_string();
            let state = parts[1].to_string();
            if serial == "no" && state == "permissions" {
                continue;
            }

            let mut model = String::new();
            for p in parts.iter().skip(2) {
                if let Some(rest) = p.strip_prefix("model:") {
                    model = rest.to_string();
                    break;
                }
            }
            if model.is_empty() {
                model = serial.clone();
            }

            devices.push(Device {
                serial,
                state,
                model,
            });
        }

        Ok(devices)
    }

    /// Relevant getprop keys for device info display.
    #[allow(dead_code)]
    pub fn get_device_props(&self, serial: &str) -> Result<HashMap<String, String>> {
        let output = self.run_command(&["-s", serial, "shell", "getprop"])?;
        let stdout = String::from_utf8_lossy(&output.stdout);
        let mut info = HashMap::new();
        let relevant_keys = [
            "ro.product.model",
            "ro.product.manufacturer",
            "ro.build.version.release",
            "ro.build.version.sdk",
        ];
        for line in stdout.lines() {
            if let Some((key, value)) = line.split_once("]: [") {
                let key = key.trim_start_matches('[');
                let value = value.trim_end_matches(']');
                if relevant_keys.contains(&key) {
                    info.insert(key.to_string(), value.to_string());
                }
            }
        }
        Ok(info)
    }

    #[allow(dead_code)]
    pub fn install_apk(&self, device: &str, apk_path: &Path) -> Result<()> {
        if !apk_path.exists() {
            return Err(anyhow!("APK does not exist: {}", apk_path.display()));
        }
        let output = self.run_command(&[
            "-s",
            device,
            "install",
            "-r",
            "-d",
            &apk_path.to_string_lossy(),
        ])?;
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        if stdout.contains("Success") {
            Ok(())
        } else {
            Err(anyhow!(
                "Install failed: {} {}",
                stderr.trim(),
                stdout.trim()
            ))
        }
    }

    #[allow(dead_code)]
    pub fn uninstall_package(&self, device: &str, package: &str) -> Result<()> {
        let output = self.run_command(&["-s", device, "uninstall", package])?;
        let stdout = String::from_utf8_lossy(&output.stdout);
        if stdout.contains("Success") {
            Ok(())
        } else {
            Err(anyhow!("Uninstall failed: {}", stdout.trim()))
        }
    }

    /// Find PIDs for processes matching package pattern (rustycat-style).
    pub fn pids_for_package(&self, device: &str, pattern: &str) -> Result<Vec<String>> {
        let regex_pattern = pattern.replace('.', "\\.").replace('*', ".*");
        let re = regex::Regex::new(&regex_pattern)?;
        let output = self.run_command(&["-s", device, "shell", "ps", "-A"])?;
        let processes = String::from_utf8_lossy(&output.stdout);
        let mut pids = Vec::new();
        for line in processes.lines() {
            if re.is_match(line) {
                if let Some(pid) = line.split_whitespace().nth(1) {
                    pids.push(pid.to_string());
                }
            }
        }
        Ok(pids)
    }
}
