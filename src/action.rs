//! User actions dispatched from key events.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Quit,
    // Global
    RefreshDevices,
    // Logcat
    ToggleLogcat,
    FocusFilter,
    ClearLogs,
    ToggleLogcatPause,
    TogglePackageFilter,
    ScrollUp,
    ScrollDown,
    ScrollPageUp,
    ScrollPageDown,
    ScrollTail,
    // Build
    BuildDebug,
    InstallDebug,
    RunApp,
    ToggleBuildExpand,
    // Popups
    OpenVariantPicker,
    OpenDevicePicker,
    OpenBuildPopup,
    OpenPackagePicker,
    // Shared popup navigation
    PickerNext,
    PickerPrev,
    PickerConfirm,
    PickerCancel,
    // Misc
    LaunchScrcpy,
    StopProcess,
    ConfirmYes,
    ConfirmNo,
}
