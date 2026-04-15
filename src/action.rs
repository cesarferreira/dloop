//! User actions dispatched from key events.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Quit,
    // Logcat
    ToggleLogcat,
    FocusFilter,
    ClearFilter,
    FocusExclude,
    ClearExclude,
    ClearLogs,
    ToggleLogcatPause,
    TogglePackageFilter,
    OpenLevelPicker,
    ScrollUp,
    ScrollDown,
    ScrollPageUp,
    ScrollPageDown,
    ScrollTail,
    ExportLogs,
    OpenCrashDetail,
    CrashCopy,
    CrashAgent,
    CrashExport,
    CrashSearch,
    // Build
    BuildDebug,
    InstallDebug,
    RunApp,
    // Popups
    OpenVariantPicker,
    OpenDevicePicker,
    OpenBuildPopup,
    OpenPackagePicker,
    OpenBuildHistory,
    // Shared popup navigation
    PickerNext,
    PickerPrev,
    PickerConfirm,
    PickerCancel,
    // Misc
    LaunchScrcpy,
    StopProcess,
    ConfirmNo,
    OpenHelp,
}
