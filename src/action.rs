//! User actions dispatched from key events.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Quit,
    RefreshDevices,
    NextPane,
    PrevPane,
    // Devices pane
    NextDevice,
    PrevDevice,
    // Logcat pane
    ToggleLogcat,
    FocusFilter,
    ClearLogs,
    ToggleLogcatPause,
    TogglePackageFilter,   // 'a' — all logs vs filtered by package
    ScrollUp,
    ScrollDown,
    ScrollPageUp,
    ScrollPageDown,
    ScrollTail,            // End / 'G' — jump back to tail
    // Build pane
    BuildDebug,
    InstallDebug,
    RunApp,                // install + launch the app
    ToggleBuildExpand,     // 'e' — expand/collapse build output
    OpenVariantPicker,     // 'v' — pick build variant
    StopProcess,
    LaunchScrcpy,
    // Variant picker navigation
    PickerNext,
    PickerPrev,
    PickerConfirm,
    PickerCancel,
    // Misc
    ConfirmYes,
    ConfirmNo,
}
