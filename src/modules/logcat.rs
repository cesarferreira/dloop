//! Logcat line parsing and filtering (rustycat-inspired).
use crate::adb::AdbClient;
use anyhow::Result;
use std::collections::HashMap;
use std::io::{BufRead, BufReader};
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::Sender;
use std::thread;

#[derive(Debug, Clone)]
pub struct LogEntry {
    pub raw: String,
    pub timestamp: String,
    pub pid: String,
    pub tag: String,
    pub level: String,
    pub message: String,
    /// First line of a detected crash / ANR block (for UI highlight).
    pub crash_start: bool,
    /// Pre-computed flag for stack trace detection (performance optimization).
    pub is_stack_trace: bool,
    /// Pre-computed searchable content (tag + level + message + raw, lowercased).
    /// Used for filter and exclude matching to avoid repeated allocations.
    pub cached_search_text: String,
}

/// Parse standard `logcat -v time` line.
pub fn parse_log_line(line: &str) -> Option<LogEntry> {
    let parts: Vec<&str> = line.split_whitespace().collect();
    if parts.len() < 6 {
        return None;
    }

    let time_parts: Vec<&str> = parts[1].split('.').collect();
    let time = time_parts[0];
    let ms_raw = time_parts.get(1).unwrap_or(&"000");
    let mut ms_fixed: String = ms_raw.chars().take(3).collect();
    while ms_fixed.len() < 3 {
        ms_fixed.push('0');
    }
    let timestamp = format!("{}.{}", time, ms_fixed);

    let pid = parts[2].to_string();
    let level = parts[4].to_string();
    let tag_and_message = parts[5..].join(" ");
    let (tag, message) = if let Some(pos) = tag_and_message.find(": ") {
        let (t, m) = tag_and_message.split_at(pos);
        (
            t.trim().trim_end_matches(':').to_string(),
            m.trim_start_matches(": ").to_string(),
        )
    } else {
        (tag_and_message.clone(), String::new())
    };

    let is_stack_trace = looks_like_stack_trace(&message) || looks_like_stack_trace(line);

    // Pre-compute searchable content once (huge perf win)
    let cached_search_text = format!("{} {} {} {}", tag, level, message, line).to_lowercase();

    Some(LogEntry {
        raw: line.to_string(),
        timestamp,
        pid,
        tag,
        level,
        message,
        crash_start: false,
        is_stack_trace,
        cached_search_text,
    })
}

/// A captured crash / ANR block (cloned lines for export / yank).
#[derive(Debug, Clone)]
pub struct CrashEvent {
    pub timestamp: String,
    pub summary: String,
    pub lines: Vec<LogEntry>,
}

/// Detects the first line of a Java crash or ANR in logcat.
pub fn is_crash_start(entry: &LogEntry) -> bool {
    let m = entry.message.to_lowercase();
    let r = entry.raw.to_lowercase();
    m.contains("fatal exception")
        || r.contains("fatal exception")
        || m.contains("anr in ")
        || r.contains("anr in ")
        || m.contains("crash:")
        || r.contains("crash:")
}

/// Lines that belong to the same crash stack trace / follow-up.
pub fn is_crash_continuation(entry: &LogEntry) -> bool {
    if is_crash_start(entry) {
        return false;
    }
    looks_like_stack_trace(&entry.message)
        || looks_like_stack_trace(&entry.raw)
        || entry.message.trim_start().starts_with("at ")
        || entry.message.contains("Caused by:")
        || entry.message.contains("Suppressed:")
        || entry.tag == "AndroidRuntime"
        || entry.tag == "System.err"
}

pub fn level_style(level: &str) -> ratatui::style::Color {
    use ratatui::style::Color::*;
    match level {
        "D" => Cyan,
        "I" => Green,
        "W" => Yellow,
        "E" => Red,
        "V" => Blue,
        "F" => Red,
        _ => Gray,
    }
}

const TAG_COLORS: &[ratatui::style::Color] = &[
    ratatui::style::Color::Red,
    ratatui::style::Color::Green,
    ratatui::style::Color::Yellow,
    ratatui::style::Color::Blue,
    ratatui::style::Color::Magenta,
    ratatui::style::Color::Cyan,
    ratatui::style::Color::LightRed,
    ratatui::style::Color::LightGreen,
    ratatui::style::Color::LightYellow,
    ratatui::style::Color::LightBlue,
    ratatui::style::Color::LightMagenta,
    ratatui::style::Color::LightCyan,
];

pub fn tag_color(tag: &str, cache: &mut HashMap<String, usize>) -> ratatui::style::Color {
    let idx = if let Some(&i) = cache.get(tag) {
        i
    } else {
        let i = cache.len();
        cache.insert(tag.to_string(), i);
        i
    };
    TAG_COLORS[idx % TAG_COLORS.len()]
}

pub struct LogcatFilter {
    /// When true and `pids` is non-empty, only lines whose PID is in `pids` pass.
    pub filter_by_application_ids: bool,
    pub tag_substrings: Vec<String>,
    pub levels: Option<String>, // e.g. "D,I,W,E"
    pub content: Option<String>,
    /// If any substring matches tag/message/raw (case-insensitive), the line is dropped.
    pub exclude_substrings: Vec<String>,
}

impl LogcatFilter {
    pub fn allows(&self, entry: &LogEntry, pids: &[String]) -> bool {
        if self.filter_by_application_ids && !pids.is_empty() && !pids.contains(&entry.pid) {
            return false;
        }
        if !self.tag_substrings.is_empty() {
            let tag_lower = entry.tag.to_lowercase();
            let any = self
                .tag_substrings
                .iter()
                .any(|t| !t.is_empty() && tag_lower.contains(&t.to_lowercase()));
            if !any {
                return false;
            }
        }
        if !matches_level_filter(self.levels.as_deref(), &entry.level) {
            return false;
        }
        if let Some(ref c) = self.content {
            if !c.is_empty() {
                let hay = format!("{} {} {}", entry.tag, entry.message, entry.raw).to_lowercase();
                if !hay.contains(&c.to_lowercase()) {
                    return false;
                }
            }
        }
        if !self.exclude_substrings.is_empty() {
            let hay = format!("{} {} {}", entry.tag, entry.message, entry.raw).to_lowercase();
            for e in &self.exclude_substrings {
                if !e.is_empty() && hay.contains(&e.to_lowercase()) {
                    return false;
                }
            }
        }
        true
    }
}

pub fn matches_level_filter(levels: Option<&str>, level: &str) -> bool {
    let Some(levels) = levels else {
        return true;
    };
    if levels.is_empty() {
        return true;
    }
    levels.split(',').any(|x| x.trim() == level)
}

pub fn spawn_logcat_reader(adb_path: &Path, device: &str, tx: Sender<String>) -> Result<Child> {
    let mut child = Command::new(adb_path)
        .args(["-s", device, "logcat", "-v", "threadtime"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;

    let stdout = child.stdout.take().expect("stdout");
    let stderr = child.stderr.take().expect("stderr");
    let tx_err = tx.clone();
    thread::spawn(move || {
        let mut reader = BufReader::new(stdout);
        let mut line = String::new();
        loop {
            line.clear();
            match reader.read_line(&mut line) {
                Ok(0) => break,
                Ok(_) => {
                    let t = line.trim_end().to_string();
                    if tx.send(t).is_err() {
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
                    let _ = tx_err.send(format!("[adb logcat stderr] {t}"));
                }
                Err(_) => break,
            }
        }
    });

    Ok(child)
}

/// Resolve PIDs for one package pattern (may be empty if app not running).
pub fn refresh_pids(adb: &AdbClient, device: &str, pattern: &str) -> Result<Vec<String>> {
    if pattern.is_empty() {
        return Ok(vec![]);
    }
    adb.pids_for_package(device, pattern)
}

/// Merge PIDs for all application IDs (e.g. `ai.wayve.app` and `ai.wayve.app.dev`).
pub fn refresh_pids_for_packages(
    adb: &AdbClient,
    device: &str,
    package_ids: &[String],
) -> Result<Vec<String>> {
    let mut out = Vec::new();
    for id in package_ids {
        if id.is_empty() {
            continue;
        }
        out.extend(refresh_pids(adb, device, id)?);
    }
    out.sort();
    out.dedup();
    Ok(out)
}

/// Clear logcat buffer on device.
pub fn clear_buffer(adb: &AdbClient, device: &str) -> Result<()> {
    adb.run_command(&["-s", device, "logcat", "-c"])?;
    Ok(())
}

/// Optimized version using cached search text to avoid string allocations.
pub fn matches_any_exclude_with_cached(excludes: &[String], entry: &LogEntry) -> bool {
    if excludes.is_empty() {
        return false;
    }
    excludes
        .iter()
        .any(|e| !e.is_empty() && entry.cached_search_text.contains(&e.to_lowercase()))
}

pub fn looks_like_stack_trace(line: &str) -> bool {
    line.trim_start().starts_with("at ")
        || line.contains("Exception")
        || line.contains("Error:")
        || line.trim_start().starts_with("Caused by:")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_sample_logcat_line() {
        let line = "02-03 15:44:41.704  2359  3654 I MyTag: hello world";
        let e = parse_log_line(line).expect("parse");
        assert_eq!(e.level, "I");
        assert_eq!(e.pid, "2359");
        assert_eq!(e.tag, "MyTag");
        assert_eq!(e.message, "hello world");
    }

    #[test]
    fn exclude_substrings_drop_matching_lines() {
        let f = LogcatFilter {
            filter_by_application_ids: false,
            tag_substrings: vec![],
            levels: None,
            content: None,
            exclude_substrings: vec!["chatty".to_string()],
        };
        let e = parse_log_line("02-03 15:44:41.704  2359  3654 I chatty: blah").expect("parse");
        assert!(!f.allows(&e, &[]));
        let ok = parse_log_line("02-03 15:44:41.704  2359  3654 I MyTag: hello").expect("parse");
        assert!(f.allows(&ok, &[]));
    }

    #[test]
    fn level_filter_matches_expected_levels() {
        assert!(matches_level_filter(None, "E"));
        assert!(matches_level_filter(Some("E,F"), "E"));
        assert!(matches_level_filter(Some("E,F"), "F"));
        assert!(!matches_level_filter(Some("E,F"), "W"));
    }
}
