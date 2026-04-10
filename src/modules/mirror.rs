//! Launch scrcpy for the selected device.
use anyhow::{anyhow, Result};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use which::which;

pub fn launch_scrcpy(serial: &str, extra_args: &[String]) -> Result<()> {
    let scrcpy = which("scrcpy").map_err(|_| anyhow!("scrcpy not found in PATH"))?;
    let mut cmd = Command::new(scrcpy);
    cmd.arg("-s").arg(serial);
    for a in extra_args {
        cmd.arg(a);
    }
    cmd.stdin(Stdio::null());
    cmd.stdout(Stdio::null());
    cmd.stderr(Stdio::null());
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        cmd.process_group(0);
    }
    cmd.spawn()
        .map_err(|e| anyhow!("failed to spawn scrcpy: {e}"))?;
    Ok(())
}

#[allow(dead_code)]
pub fn scrcpy_path() -> Option<PathBuf> {
    which("scrcpy").ok()
}
