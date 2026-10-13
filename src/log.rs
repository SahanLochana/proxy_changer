use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::Mutex;
use windows::Win32::System::SystemInformation::GetLocalTime;
use serde::{Deserialize, Serialize};

static LOG_MUTEX: Mutex<()> = Mutex::new(());

const MAX_LOG_SIZE: u64 = 512 * 1024; // 512 KB

pub fn app_data_dir() -> PathBuf {
    let app_data = std::env::var("APPDATA").unwrap_or_else(|_| ".".to_string());
    let dir = PathBuf::from(app_data).join("WifiProxySwitcher");
    let _ = fs::create_dir_all(&dir);
    dir
}

pub fn log_path() -> PathBuf {
    app_data_dir().join("app.log")
}

pub fn log_old_path() -> PathBuf {
    app_data_dir().join("app.log.old")
}

fn current_timestamp() -> String {
    unsafe {
        let st = GetLocalTime();
        format!(
            "{:04}-{:02}-{:02} {:02}:{:02}:{:02}",
            st.wYear, st.wMonth, st.wDay, st.wHour, st.wMinute, st.wSecond
        )
    }
}

pub fn log(level: &str, msg: &str) {
    let _guard = LOG_MUTEX.lock().unwrap_or_else(|e| e.into_inner());

    let path = log_path();

    // Check size and trim if necessary (> 512 KB)
    if let Ok(metadata) = fs::metadata(&path) {
        if metadata.len() >= MAX_LOG_SIZE {
            let old_path = log_old_path();
            let _ = fs::remove_file(&old_path);
            let _ = fs::rename(&path, &old_path);
        }
    }

    let timestamp = current_timestamp();
    let line = format!("{} [{}] {}\n", timestamp, level, msg);

    if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(&path) {
        let _ = file.write_all(line.as_bytes());
    }
}

#[macro_export]
macro_rules! log_info {
    ($($arg:tt)*) => {
        $crate::log::log("INFO", &format!($($arg)*))
    };
}

#[macro_export]
macro_rules! log_warn {
    ($($arg:tt)*) => {
        $crate::log::log("WARN", &format!($($arg)*))
    };
}

#[macro_export]
macro_rules! log_error {
    ($($arg:tt)*) => {
        $crate::log::log("ERROR", &format!($($arg)*))
    };
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct LogEntry {
    pub timestamp: String,
    pub level: String,
    pub message: String,
}

pub fn parse_log_line(line: &str) -> Option<LogEntry> {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return None;
    }
    if let Some(open_idx) = trimmed.find('[') {
        if let Some(close_idx) = trimmed[open_idx..].find(']') {
            let actual_close = open_idx + close_idx;
            let timestamp = trimmed[..open_idx].trim().to_string();
            let level = trimmed[open_idx + 1..actual_close].trim().to_uppercase();
            let message = trimmed[actual_close + 1..].trim().to_string();
            return Some(LogEntry {
                timestamp,
                level,
                message,
            });
        }
    }
    Some(LogEntry {
        timestamp: String::new(),
        level: "INFO".to_string(),
        message: trimmed.to_string(),
    })
}

pub fn read_logs() -> Vec<LogEntry> {
    let _guard = LOG_MUTEX.lock().unwrap_or_else(|e| e.into_inner());
    let path = log_path();
    let content = match fs::read_to_string(&path) {
        Ok(c) => c,
        Err(_) => return Vec::new(),
    };

    let mut entries = Vec::new();
    for line in content.lines() {
        if let Some(entry) = parse_log_line(line) {
            entries.push(entry);
        }
    }
    entries
}

pub fn clear_logs() -> std::io::Result<()> {
    let _guard = LOG_MUTEX.lock().unwrap_or_else(|e| e.into_inner());
    let path = log_path();
    if path.exists() {
        fs::write(&path, "")?;
    }
    let old_path = log_old_path();
    if old_path.exists() {
        let _ = fs::remove_file(&old_path);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_log_line() {
        let line = "2026-10-14 00:14:45 [INFO] Starting Wi-Fi Proxy Switcher";
        let entry = parse_log_line(line).unwrap();
        assert_eq!(entry.timestamp, "2026-10-14 00:14:45");
        assert_eq!(entry.level, "INFO");
        assert_eq!(entry.message, "Starting Wi-Fi Proxy Switcher");

        let warn_line = "2026-10-14 00:15:00 [WARN] WLAN client warning";
        let entry = parse_log_line(warn_line).unwrap();
        assert_eq!(entry.level, "WARN");
        assert_eq!(entry.message, "WLAN client warning");

        let err_line = "2026-10-14 00:15:10 [ERROR] Failed to apply proxy: timeout";
        let entry = parse_log_line(err_line).unwrap();
        assert_eq!(entry.level, "ERROR");
        assert_eq!(entry.message, "Failed to apply proxy: timeout");

        let raw_line = "Unformatted message";
        let entry = parse_log_line(raw_line).unwrap();
        assert_eq!(entry.message, "Unformatted message");
        assert_eq!(entry.level, "INFO");

        assert!(parse_log_line("   ").is_none());
    }
}
