//! Device list refresh (thin wrapper over AdbClient).
use crate::adb::{AdbClient, Device};
use anyhow::Result;

pub fn scan_devices(adb: &AdbClient) -> Result<Vec<Device>> {
    adb.list_devices()
}
