//! Crossterm event → Action mapping.
use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use std::time::Duration;

use crate::action::Action;

#[derive(Debug, Clone)]
pub enum AppEvent {
    Action(Action),
    Text(char),
    Backspace,
}

/// Which modal is active — determines key routing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Modal {
    None,
    Filter,
    ExcludeFilter,
    LevelPicker,
    VariantPicker,
    DevicePicker,
    BuildPopup,
    PackagePicker,
    BuildHistory,
    CrashDetail,
    HelpPopup,
}

pub fn poll_event(timeout: Duration, modal: Modal) -> std::io::Result<Option<AppEvent>> {
    if !event::poll(timeout)? {
        return Ok(None);
    }
    match event::read()? {
        Event::Key(key) => {
            if key.kind == KeyEventKind::Release {
                return Ok(None);
            }
            Ok(match modal {
                Modal::Filter => map_filter(key.code, key.modifiers),
                Modal::ExcludeFilter => map_exclude_filter(key.code, key.modifiers),
                Modal::LevelPicker | Modal::VariantPicker | Modal::DevicePicker => {
                    map_picker(key.code)
                }
                Modal::BuildPopup => map_build_popup(key.code),
                Modal::PackagePicker => map_package_picker(key.code, key.modifiers),
                Modal::BuildHistory => map_build_history(key.code),
                Modal::CrashDetail => map_crash_detail(key.code),
                Modal::HelpPopup => map_help_popup(key.code),
                Modal::None => map_normal(key.code, key.modifiers),
            })
        }
        Event::Resize(_, _) => Ok(None),
        _ => Ok(None),
    }
}

fn map_help_popup(code: KeyCode) -> Option<AppEvent> {
    let a = match code {
        KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char('?') => Action::PickerCancel,
        _ => return None,
    };
    Some(AppEvent::Action(a))
}

fn map_picker(code: KeyCode) -> Option<AppEvent> {
    let a = match code {
        KeyCode::Up | KeyCode::Char('k') | KeyCode::Char('K') => Action::PickerPrev,
        KeyCode::Down | KeyCode::Char('j') | KeyCode::Char('J') => Action::PickerNext,
        KeyCode::Enter => Action::PickerConfirm,
        KeyCode::Esc | KeyCode::Char('q') => Action::PickerCancel,
        _ => return None,
    };
    Some(AppEvent::Action(a))
}

fn map_build_popup(code: KeyCode) -> Option<AppEvent> {
    let a = match code {
        KeyCode::Esc | KeyCode::Char('e') | KeyCode::Char('q') => Action::PickerCancel,
        KeyCode::Up | KeyCode::Char('k') => Action::ScrollUp,
        KeyCode::Down | KeyCode::Char('j') => Action::ScrollDown,
        KeyCode::PageUp => Action::ScrollPageUp,
        KeyCode::PageDown => Action::ScrollPageDown,
        _ => return None,
    };
    Some(AppEvent::Action(a))
}

fn map_crash_detail(code: KeyCode) -> Option<AppEvent> {
    let a = match code {
        KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char('Q') => Action::PickerCancel,
        KeyCode::Char('c') | KeyCode::Char('C') => Action::CrashCopy,
        KeyCode::Char('a') | KeyCode::Char('A') => Action::CrashAgent,
        KeyCode::Char('w') | KeyCode::Char('W') => Action::CrashExport,
        KeyCode::Char('s') | KeyCode::Char('S') => Action::CrashSearch,
        KeyCode::Up | KeyCode::Char('k') | KeyCode::Char('K') => Action::ScrollUp,
        KeyCode::Down | KeyCode::Char('j') | KeyCode::Char('J') => Action::ScrollDown,
        KeyCode::PageUp => Action::ScrollPageUp,
        KeyCode::PageDown => Action::ScrollPageDown,
        _ => return None,
    };
    Some(AppEvent::Action(a))
}

fn map_build_history(code: KeyCode) -> Option<AppEvent> {
    let a = match code {
        KeyCode::Esc | KeyCode::Char('q') => Action::PickerCancel,
        KeyCode::Up | KeyCode::Char('k') => Action::PickerPrev,
        KeyCode::Down | KeyCode::Char('j') => Action::PickerNext,
        KeyCode::PageUp => Action::ScrollPageUp,
        KeyCode::PageDown => Action::ScrollPageDown,
        _ => return None,
    };
    Some(AppEvent::Action(a))
}

fn map_package_picker(code: KeyCode, modifiers: KeyModifiers) -> Option<AppEvent> {
    match code {
        KeyCode::Esc => Some(AppEvent::Action(Action::PickerCancel)),
        KeyCode::Enter => Some(AppEvent::Action(Action::PickerConfirm)),
        KeyCode::Up => Some(AppEvent::Action(Action::PickerPrev)),
        KeyCode::Down => Some(AppEvent::Action(Action::PickerNext)),
        KeyCode::Backspace => Some(AppEvent::Backspace),
        KeyCode::Char(c) if !modifiers.contains(KeyModifiers::CONTROL) => Some(AppEvent::Text(c)),
        _ => None,
    }
}

fn map_filter(code: KeyCode, modifiers: KeyModifiers) -> Option<AppEvent> {
    match code {
        KeyCode::Esc => Some(AppEvent::Action(Action::ClearFilter)),
        KeyCode::Enter => Some(AppEvent::Action(Action::FocusFilter)),
        KeyCode::Backspace => Some(AppEvent::Backspace),
        KeyCode::Char(c) if !modifiers.contains(KeyModifiers::CONTROL) => Some(AppEvent::Text(c)),
        _ => None,
    }
}

fn map_exclude_filter(code: KeyCode, modifiers: KeyModifiers) -> Option<AppEvent> {
    match code {
        KeyCode::Esc => Some(AppEvent::Action(Action::ClearExclude)),
        KeyCode::Enter => Some(AppEvent::Action(Action::FocusExclude)),
        KeyCode::Backspace => Some(AppEvent::Backspace),
        KeyCode::Char(c) if !modifiers.contains(KeyModifiers::CONTROL) => Some(AppEvent::Text(c)),
        _ => None,
    }
}

fn map_normal(code: KeyCode, modifiers: KeyModifiers) -> Option<AppEvent> {
    let action = match code {
        KeyCode::Char('q') | KeyCode::Char('Q') => Some(Action::Quit),
        KeyCode::Esc => Some(Action::ConfirmNo),

        // Scroll (logcat is always the main pane)
        KeyCode::Up | KeyCode::Char('k') | KeyCode::Char('K')
            if !modifiers.contains(KeyModifiers::CONTROL) =>
        {
            Some(Action::ScrollUp)
        }
        KeyCode::Down | KeyCode::Char('j') | KeyCode::Char('J')
            if !modifiers.contains(KeyModifiers::CONTROL) =>
        {
            Some(Action::ScrollDown)
        }
        KeyCode::PageUp => Some(Action::ScrollPageUp),
        KeyCode::PageDown => Some(Action::ScrollPageDown),
        KeyCode::End => Some(Action::ScrollTail),
        KeyCode::Char('G') => Some(Action::ScrollTail),

        // Build
        KeyCode::Char('b') if !modifiers.contains(KeyModifiers::CONTROL) => {
            Some(Action::BuildDebug)
        }
        KeyCode::Char('i') if !modifiers.contains(KeyModifiers::CONTROL) => {
            Some(Action::InstallDebug)
        }
        KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Char('r') | KeyCode::Char('R') => {
            Some(Action::RunApp)
        }

        // Logcat
        KeyCode::Char('l') => Some(Action::ToggleLogcat),
        KeyCode::Char('L') => Some(Action::OpenLevelPicker),
        KeyCode::Char('f') | KeyCode::Char('F') => Some(Action::FocusFilter),
        KeyCode::Char('x') | KeyCode::Char('X') => Some(Action::FocusExclude),
        KeyCode::Char('c') if !modifiers.contains(KeyModifiers::CONTROL) => Some(Action::ClearLogs),
        KeyCode::Char(' ') => Some(Action::ToggleLogcatPause),
        KeyCode::Char('a') | KeyCode::Char('A') => Some(Action::TogglePackageFilter),

        // Popups
        KeyCode::Char('d') | KeyCode::Char('D') => Some(Action::OpenDevicePicker),
        KeyCode::Char('v') | KeyCode::Char('V') => Some(Action::OpenVariantPicker),
        KeyCode::Char('e') | KeyCode::Char('E') => Some(Action::OpenBuildPopup),
        KeyCode::Char('p') | KeyCode::Char('P') => Some(Action::OpenPackagePicker),
        KeyCode::Char('H') | KeyCode::Char('h') => Some(Action::OpenBuildHistory),

        // Other
        KeyCode::Char('m') | KeyCode::Char('M') => Some(Action::LaunchScrcpy),
        KeyCode::Char('s') if !modifiers.contains(KeyModifiers::CONTROL) => {
            Some(Action::StopProcess)
        }
        // r/R and n/N both trigger RunApp (see above); no explicit refresh binding needed
        // since device detection is automatic.
        KeyCode::Char('w') | KeyCode::Char('W') => Some(Action::ExportLogs),
        KeyCode::Char('y') | KeyCode::Char('Y') => Some(Action::OpenCrashDetail),
        KeyCode::Char('?') => Some(Action::OpenHelp),

        _ => None,
    };
    action.map(AppEvent::Action)
}
