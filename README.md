# Wi-Fi Proxy Auto-Switcher

A lightweight, native Windows system tray application written in Rust that automatically configures Windows system proxy settings based on the currently connected Wi-Fi SSID.

---

## Features

- **Automatic Wi-Fi SSID Detection:** Monitors network state transitions in real time using Windows Native WLAN API (`WlanRegisterNotification`).
- **Configurable Rules:** Define per-SSID proxy rules (`host:port`).
- **Unmatched Behaviors:** Choose between disabling the proxy, keeping the current proxy, or applying a fallback default proxy when connected to unknown networks or disconnected.
- **Manual Overrides:** Immediate "Apply Proxy Now" and "Clear Proxy" buttons in both the main window and the system tray menu.
- **Debounced Updates:** Debounces rapid Wi-Fi notifications (500 ms) before querying the network state.
- **Redundant Write Prevention:** Compares target proxy state against current registry settings before writing, avoiding unnecessary system notifications.
- **Modern Windows 11 Dark Aesthetic:**
  - Sleek dark theme UI with rounded cards, custom title bar, glowing toggle switches, and subtle borders.
  - Live Connection Status card showing current Wi-Fi SSID and active proxy state badges.
  - Interactive table of rules with inline selection, editing, and discovered SSID dropdown.
  - Fallback radio options and quick action buttons ("Apply Proxy Now", "Clear Proxy", "Open Log File").
- **Lightweight & Efficient:**
  - Standalone `.exe` binary.
  - Idle CPU ~0% (event-driven, no fast polling loops).
  - No console window (`#![windows_subsystem = "windows"]`).
- **Zero UAC / No Admin Required:** Uses only `HKCU` (`HKCU\Software\Microsoft\Windows\CurrentVersion\Internet Settings` and `HKCU\Software\Microsoft\Windows\CurrentVersion\Run`).
- **System Tray Integration:**
  - Left-click tray icon to show/restore the window.
  - Right-click tray menu: *Open Settings*, *Auto-Switch* (with checkmark), *Apply Proxy Now*, *Clear Proxy*, *Quit*.
  - Tray tooltip shows current SSID, active proxy, and auto-switch state.
  - Dynamic tray icon reflects active vs disabled state.
  - Native balloon notifications on proxy changes (can be toggled on/off).
- **Auto-Start with Windows:** Toggle in the UI writes/removes `"<exe path>" --hidden` to HKCU Run key.
- **Single Instance:** Enforces single instance via a named mutex (`Global\WifiProxySwitcher_SingleInstance_Mutex`). Second launches restore the existing window and exit cleanly.
- **Self-Trimming Logging:** Writes structured logs to `%APPDATA%\WifiProxySwitcher\app.log` with automatic rollover at 512 KB to `app.log.old`.

---

## Project Structure

```
src/
  ├── main.rs      // Entry point, single-instance mutex, --hidden flag, Tao event loop
  ├── wifi.rs      // WLAN API client, SSID query, interface enumeration, notification callback
  ├── proxy.rs     // Windows Internet Settings registry reader, applier, clearer, WinInet notification
  ├── config.rs    // Configuration struct, JSON persistence, validation, unit tests
  ├── logic.rs     // Decision matrix (SSID + config -> Action) & redundant write suppression
  ├── ui.rs        // Tao borderless window, Wry WebView2, tray-icon, IPC message handling
  ├── autostart.rs // HKCU Run key management
  └── log.rs       // Thread-safe file logger with automatic size trimming
assets/
  └── index.html   // Windows 11 Fluent dark theme UI (cards, toggle switch, table, badges)
build.rs           // Asset generation & Windows resource embedding (winres)
```

---

## Build Instructions

Requires Rust (stable toolchain) on Windows 10/11.

```powershell
# Run unit tests
cargo test

# Build debug binary
cargo build

# Build optimized release binary (stripped, LTO enabled)
cargo build --release
```

The compiled release binary is located at:
```
target/release/proxy_changer.exe
```

---

## Running the Application

### Normal Mode (GUI window shown on startup)
```powershell
.\target\release\proxy_changer.exe
```

### Background / Tray-Only Mode (Used by Windows Autostart)
```powershell
.\target\release\proxy_changer.exe --hidden
```

---

## Configuration & Logs

All application data is stored in the standard user directory:
```
%APPDATA%\WifiProxySwitcher\
  ├── config.json   // Rules, master toggle, unmatched behavior, options
  ├── app.log       // Active log file
  └── app.log.old   // Rollover log file (trimmed when app.log exceeds 512 KB)
```

### Example `config.json`
```json
{
  "enabled": true,
  "rules": [
    {
      "ssid": "Office_5G",
      "proxy": "10.0.0.1:8080"
    },
    {
      "ssid": "Campus_WiFi",
      "proxy": "proxy.corp.internal:3128"
    }
  ],
  "unmatched": "disable",
  "default_proxy": "",
  "notify": true,
  "start_with_windows": false
}
```

---

## Known Limits & Notes

- **Windows 11 24H2 Location Permission:** Windows 11 24H2 may require Location permission for standard desktop applications to query the Wi-Fi SSID name. If connected to a network but the SSID cannot be read due to OS privacy settings, the status panel will display `SSID unavailable (check Location permission)` and log the condition.
- **Case-Sensitive SSIDs:** As specified in the 802.11 standard, SSID names are matched case-sensitively.
