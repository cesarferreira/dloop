//! Crossterm event → Action mapping with pane context.
use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use std::time::Duration;

use crate::action::Action;
use crate::app::Pane;

#[derive(Debug, Clone)]
pub enum AppEvent {
    Action(Action),
    Text(char),
    Backspace,
}

pub fn poll_event(
    timeout: Duration,
    filter_focused: bool,
    active_pane: Pane,
    picker_open: bool,
) -> std::io::Result<Option<AppEvent>> {
    if !event::poll(timeout)? {
        return Ok(None);
    }
    match event::read()? {
        Event::Key(key) => {
            if key.kind == KeyEventKind::Release {
                return Ok(None);
            }
            if picker_open {
                return Ok(map_key_picker(key.code));
            }
            if filter_focused {
                return Ok(map_key_filter_mode(key.code, key.modifiers));
            }
            Ok(map_key(key.code, key.modifiers, active_pane))
        }
        Event::Resize(_, _) => Ok(None),
        _ => Ok(None),
    }
}

fn map_key_picker(code: KeyCode) -> Option<AppEvent> {
    let a = match code {
        KeyCode::Up | KeyCode::Char('k') | KeyCode::Char('K') => Action::PickerPrev,
        KeyCode::Down | KeyCode::Char('j') | KeyCode::Char('J') => Action::PickerNext,
        KeyCode::Enter => Action::PickerConfirm,
        KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char('v') => Action::PickerCancel,
        _ => return None,
    };
    Some(AppEvent::Action(a))
}

fn map_key_filter_mode(code: KeyCode, modifiers: KeyModifiers) -> Option<AppEvent> {
    match code {
        KeyCode::Esc => Some(AppEvent::Action(Action::ConfirmNo)),
        KeyCode::Enter => Some(AppEvent::Action(Action::FocusFilter)),
        KeyCode::Tab => Some(AppEvent::Action(Action::NextPane)),
        KeyCode::Char('q') | KeyCode::Char('Q') => Some(AppEvent::Action(Action::Quit)),
        KeyCode::Backspace => Some(AppEvent::Backspace),
        KeyCode::Char(c) if !modifiers.contains(KeyModifiers::CONTROL) => Some(AppEvent::Text(c)),
        _ => None,
    }
}

fn map_key(code: KeyCode, modifiers: KeyModifiers, pane: Pane) -> Option<AppEvent> {
    let action = match code {
        KeyCode::Char('q') | KeyCode::Char('Q') => Some(Action::Quit),
        KeyCode::Tab => Some(Action::NextPane),
        KeyCode::BackTab => Some(Action::PrevPane),
        KeyCode::Esc => Some(Action::ConfirmNo),

        // Up/Down are contextual
        KeyCode::Up => Some(match pane {
            Pane::Logs => Action::ScrollUp,
            _ => Action::PrevDevice,
        }),
        KeyCode::Down => Some(match pane {
            Pane::Logs => Action::ScrollDown,
            _ => Action::NextDevice,
        }),
        KeyCode::PageUp => Some(Action::ScrollPageUp),
        KeyCode::PageDown => Some(Action::ScrollPageDown),
        KeyCode::End => Some(Action::ScrollTail),

        // vim-style scroll (always, since j/k won't conflict when in logs)
        KeyCode::Char('k') | KeyCode::Char('K')
            if pane == Pane::Logs && !modifiers.contains(KeyModifiers::CONTROL) =>
        {
            Some(Action::ScrollUp)
        }
        KeyCode::Char('j') | KeyCode::Char('J')
            if pane == Pane::Logs && !modifiers.contains(KeyModifiers::CONTROL) =>
        {
            Some(Action::ScrollDown)
        }
        KeyCode::Char('G') if pane == Pane::Logs => Some(Action::ScrollTail),

        KeyCode::Char('r') | KeyCode::Char('R') => Some(Action::RefreshDevices),
        KeyCode::Char('b') if !modifiers.contains(KeyModifiers::CONTROL) => Some(Action::BuildDebug),
        KeyCode::Char('i') if !modifiers.contains(KeyModifiers::CONTROL) => Some(Action::InstallDebug),
        KeyCode::Char('l') | KeyCode::Char('L') => Some(Action::ToggleLogcat),
        KeyCode::Char('f') | KeyCode::Char('F') => Some(Action::FocusFilter),
        KeyCode::Char('c') if !modifiers.contains(KeyModifiers::CONTROL) => Some(Action::ClearLogs),
        KeyCode::Char('m') | KeyCode::Char('M') => Some(Action::LaunchScrcpy),
        KeyCode::Char('s') if !modifiers.contains(KeyModifiers::CONTROL) => Some(Action::StopProcess),
        KeyCode::Char(' ') => Some(Action::ToggleLogcatPause),
        KeyCode::Char('a') | KeyCode::Char('A') => Some(Action::TogglePackageFilter),
        KeyCode::Char('e') | KeyCode::Char('E') => Some(Action::ToggleBuildExpand),
        KeyCode::Char('v') | KeyCode::Char('V') => Some(Action::OpenVariantPicker),
        KeyCode::Char('y') | KeyCode::Char('Y') => Some(Action::ConfirmYes),
        KeyCode::Char('n') | KeyCode::Char('N') => Some(Action::ConfirmNo),

        // Device navigation fallback when NOT in logs pane
        KeyCode::Char('k') | KeyCode::Char('K')
            if pane != Pane::Logs && !modifiers.contains(KeyModifiers::CONTROL) =>
        {
            Some(Action::PrevDevice)
        }
        KeyCode::Char('j') | KeyCode::Char('J')
            if pane != Pane::Logs && !modifiers.contains(KeyModifiers::CONTROL) =>
        {
            Some(Action::NextDevice)
        }

        _ => None,
    };
    action.map(AppEvent::Action)
}
