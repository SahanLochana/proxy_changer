#![windows_subsystem = "windows"]

pub mod autostart;
pub mod config;
pub mod log;
pub mod logic;
pub mod proxy;
pub mod ui;
pub mod wifi;

use windows::core::w;
use windows::Win32::Foundation::{GetLastError, ERROR_ALREADY_EXISTS};
use windows::Win32::System::Threading::CreateMutexW;
use windows::Win32::UI::WindowsAndMessaging::{
    FindWindowW, SetForegroundWindow, ShowWindow, SW_RESTORE,
};

fn main() {
    // 1. Single Instance check via named mutex
    let mutex_name = w!("Global\\WifiProxySwitcher_SingleInstance_Mutex");
    let mutex = unsafe { CreateMutexW(None, true, mutex_name) };
    if mutex.is_ok() && unsafe { GetLastError() } == ERROR_ALREADY_EXISTS {
        // Bring existing window to front if running
        let window_title = w!("Wi-Fi Proxy Auto-Switcher");
        let hwnd = unsafe { FindWindowW(None, window_title) };
        if let Ok(h) = hwnd {
            if !h.is_invalid() {
                unsafe {
                    let _ = ShowWindow(h, SW_RESTORE);
                    let _ = SetForegroundWindow(h);
                }
            }
        }
        return;
    }

    log_info!("Starting Wi-Fi Proxy Auto-Switcher");

    // 2. Parse command-line args for --hidden flag
    let args: Vec<String> = std::env::args().collect();
    let start_hidden = args.iter().any(|arg| arg == "--hidden");

    // 3. Initialize Native Windows GUI
    native_windows_gui::init().expect("Failed to initialize Native Windows GUI");

    // 4. Build UI and initialize application
    match ui::AppUi::build(start_hidden) {
        Ok(_app) => {
            log_info!("UI initialized successfully, entering event loop");
            // 5. Enter event loop
            native_windows_gui::dispatch_thread_events();
            log_info!("Exited event loop, shutting down");
        }
        Err(err) => {
            log_error!("Failed to build application UI: {}", err);
        }
    }
}
