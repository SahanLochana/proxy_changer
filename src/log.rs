use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::Mutex;
use windows::Win32::System::SystemInformation::GetLocalTime;

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
