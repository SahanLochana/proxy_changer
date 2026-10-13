# 📶 Wi-Fi Proxy Auto-Switcher 🚀

[![Build & Release](https://github.com/SahanLochana/proxy_changer/actions/workflows/release.yml/badge.svg)](https://github.com/SahanLochana/proxy_changer/actions/workflows/release.yml)

> ⚡ **100% Vibe Coded Application** ✨ Built with pure vibes, modern Rust, and AI pair-programming! 🤖🎧💻

A lightweight, native Windows system tray application crafted with 🦀 Rust that automatically configures Windows system proxy settings based on the currently connected Wi-Fi SSID.

---

## 🌟 Highlights & Vibe Coding DNA

- 🧘 **100% Vibe Coded:** Architected, styled, and polished entirely through prompt engineering and vibe coding excellence! 🌈✨
- 🚀 **Blazing Fast & Featherweight:** Native Rust binary, ~0% idle CPU, and zero bloat! 🪶
- 🎨 **Sleek Windows 11 Dark Aesthetic:** Gorgeous dark mode interface with custom titlebar, smooth toggle switches, and a compact 2-column layout! 🌌
- 📜 **Built-In Live Activity Logs:** Inspect real-time logs inside the app with instant level filters (`INFO`, `WARN`, `ERROR`), search, and auto-scroll—no Notepad required! 🔍

---

## ✨ Features

- 📡 **Automatic Wi-Fi SSID Detection:** Monitors network state transitions in real time using the Windows Native WLAN API (`WlanRegisterNotification`).
- 🎯 **Configurable Per-SSID Rules:** Define custom proxy endpoints (`host:port`) mapped to specific Wi-Fi networks.
- 🔀 **Smart Unmatched Behaviors:** Choose between disabling the proxy, keeping the current proxy, or applying a fallback default proxy when connected to unknown networks or disconnected.
- ⚡ **Instant Manual Overrides:** Quick "Apply Now" and "Clear Proxy" buttons in both the app dashboard and the system tray menu.
- ⏱️ **Debounced Updates:** Smooth 500 ms debouncing on rapid WLAN events prevents network flap jitter.
- 🛡️ **Redundant Write Prevention:** Compares target proxy state against current registry values before writing, avoiding unnecessary system notifications.
- 📊 **In-App Activity Logs Viewer:**
  - 📋 Live console display with timestamps, styled badges, and selectable text for easy copying.
  - 🏷️ Quick filter chips: **All**, **INFO** 🔵, **WARN** 🟡, **ERROR** 🔴.
  - 🔎 Real-time search filter for SSIDs, proxy IPs, or keywords.
  - 🔄 Auto-scroll toggle lock, manual refresh, and one-click log clearing.
- 🖥️ **Modern Windows 11 Dark UI:**
  - 🖼️ Custom embedded app logo & dynamic status badges (`enable.ico` / `disable.ico`).
  - 📶 Live Connection Status banner displaying active Wi-Fi SSID and proxy status.
  - 📝 Interactive rules manager with inline selection, editing, and discovered SSID dropdown.
  - 🚫 Scroll-free compact window (`800×500`) with no window scrollbars.
- 🔒 **Zero UAC / No Admin Privileges Required:** Operates strictly within user scope using `HKCU` (`HKCU\Software\Microsoft\Windows\CurrentVersion\Internet Settings` and `HKCU\Software\Microsoft\Windows\CurrentVersion\Run`).
- 📥 **Deep System Tray Integration:**
  - 🖱️ Left-click tray icon to show/restore window.
  - 📑 Right-click menu: *Open Settings*, *Auto-Switch* toggle, *Apply Proxy Now*, *Clear Proxy*, *Quit*.
  - 💬 Hover tooltip displays live SSID, active proxy address, and auto-switch status.
  - 🔄 Dynamic tray icon updates automatically between enabled and disabled states.
  - 🔔 Native desktop notifications on proxy changes (can be toggled on/off).
- 🚀 **Auto-Start with Windows:** Optional toggle to launch silently in the system tray on user login.
- 🛑 **Single Instance Protection:** Enforces single instance via a named mutex (`Global\WifiProxySwitcher_SingleInstance_Mutex`). Second launches bring the existing window to the front.
- 📝 **Self-Trimming File Logging:** Structured logs saved to `%APPDATA%\WifiProxySwitcher\app.log` with auto-rollover at 512 KB to `app.log.old`.

---

## 📁 Project Structure

```
src/
  ├── main.rs      // 🚪 Entry point, single-instance mutex, --hidden flag, Tao event loop
  ├── wifi.rs      // 📶 WLAN API client, SSID query, interface enumeration, notification callback
  ├── proxy.rs     // 🌐 Windows Internet Settings registry reader, applier, WinInet refresh
  ├── config.rs    // ⚙️ Configuration struct, JSON persistence, validation, unit tests
  ├── logic.rs     // 🧠 Decision matrix (SSID + config -> Action) & redundant write suppression
  ├── ui.rs        // 🖥️ Tao borderless window, Wry WebView2, tray-icon, IPC message handling
  ├── autostart.rs // 🚀 HKCU Run key management for startup
  └── log.rs       // 📜 Thread-safe logger, parser, read_logs, clear_logs & auto-trimming
assets/
  ├── app_logo.ico // 🎨 Application window & resource icon
  ├── enable.ico   // 🟢 Active proxy tray icon & status badge
  ├── disable.ico  // ⚪ Direct / disabled tray icon & status badge
  └── index.html   // 💻 Windows 11 Fluent dark UI (dashboard, rules manager, logs viewer)
build.rs           // 📦 Windows resource embedding (winres)
```

---

## 🛠️ Build Instructions

Requires **Rust** (stable toolchain) on Windows 10/11.

```powershell
# 🧪 Run all unit tests
cargo test

# 🔨 Build debug binary
cargo build

# ⚡ Build optimized production release binary (LTO + stripped)
cargo build --release
```

The compiled release binary is generated at:
```
target/release/proxy_changer.exe
```

---

## 🚀 Running the Application

### 🖥️ Normal GUI Mode
```powershell
.\target\release\proxy_changer.exe
```

### 🕶️ Background / Tray-Only Mode (Used by Windows Autostart)
```powershell
.\target\release\proxy_changer.exe --hidden
```

---

## 💾 Configuration & Logs Storage

All application data is safely saved in user AppData:
```
%APPDATA%\WifiProxySwitcher\
  ├── config.json   // ⚙️ Rules, master toggle, unmatched behavior, preferences
  ├── app.log       // 📜 Active log file
  └── app.log.old   // 📦 Rollover log file (trimmed when app.log exceeds 512 KB)
```

### 📄 Example `config.json`
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

## 💡 Notes & Best Practices

- 📍 **Windows 11 24H2 Location Permission:** Windows 11 24H2 may require Location permission for standard apps to read the Wi-Fi SSID name. If connected but the SSID cannot be read due to OS privacy settings, the status banner displays `SSID unavailable (check Location permission)` and logs the condition.
- 🔤 **Case-Sensitive SSIDs:** As specified in the IEEE 802.11 standard, SSID names are matched case-sensitively.
- ✨ **Vibe Check Passed:** 100% vibe-coded with love and focus! 💖🎉


## Readme also write using Antigravity. LoL