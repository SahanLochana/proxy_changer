# Wi-Fi Proxy Auto-Switcher: Build Spec (for coding agent)

**Goal:** Windows tray app (Rust) that sets the Windows system proxy automatically based on the connected Wi-Fi SSID.
**Instructions to agent:** Implement everything below end to end. Make reasonable decisions yourself; don't ask questions. Don't add features not listed. Keep code small, no needless abstractions. Finish with a working `cargo build --release` and the Definition of Done checklist passing.

## 1. Hard Constraints
- Windows 10/11, standard user. **No admin, no UAC.** Only HKCU.
- Language: Rust (stable), single `.exe`, no runtime deps.
- Release exe has no console: `#![windows_subsystem = "windows"]`.
- Idle RAM < 20 MB, idle CPU ~0%. **No polling loops** faster than 2 s; prefer event-driven.
- Proxy changes apply within ~2 s of a network event, with no browser/app restart.

## 2. Stack (deviate only if justified in a code comment)
| Need | Use |
|---|---|
| Win32 API | `windows` crate (features: `Win32_NetworkManagement_WiFi`, `Win32_Networking_WinInet`, `Win32_System_Registry`, `Win32_System_Threading`, `Win32_Foundation`) |
| Window, tray, menu, balloon notifications | `native-windows-gui` + `native-windows-derive` (lightweight Win32, no GPU) |
| Config | `serde`, `serde_json` |
| Paths | `%APPDATA%\WifiProxySwitcher\` (`config.json`, `app.log`) |

`Cargo.toml` release profile: `opt-level="z"`, `lto=true`, `codegen-units=1`, `panic="abort"`, `strip=true`.
Icons: two small embedded `.ico` (enabled = colored, disabled = gray), embedded via `winres` in `build.rs`, or generated at runtime.

## 3. Module Layout
```
src/
  main.rs      // entry, single-instance mutex, --hidden flag, event loop
  wifi.rs      // SSID detection, network list, WLAN notifications
  proxy.rs     // apply/clear system proxy, read current proxy
  config.rs    // Config struct, load/save, validation
  logic.rs     // decide(): SSID + config -> Action
  ui.rs        // main window, tray, menu, notifications
  autostart.rs // Run key add/remove
  log.rs       // file logger with trimming
build.rs
```

## 4. Data Model (`config.json`)
```json
{
  "enabled": true,
  "rules": [{ "ssid": "abc", "proxy": "10.0.0.1:8080" }],
  "unmatched": "disable",
  "default_proxy": "",
  "notify": true,
  "start_with_windows": false
}
```
- `unmatched`: `"disable"` | `"keep"` | `"default"` (default value `"disable"`).
- Rules: SSID unique (case-sensitive match). Save atomically (write temp file, then rename). Missing or corrupt file means defaults (log a warning).

## 5. Core Behavior

### 5.1 Wi-Fi detection (US-01) — `wifi.rs`
- `WlanOpenHandle` → `WlanEnumInterfaces` → `WlanQueryInterface(wlan_intf_opcode_current_connection)` → SSID from `WLAN_ASSOCIATION_ATTRIBUTES.dot11Ssid`.
- `current_ssid() -> Option<String>`; return `None` when there is no adapter, the interface is disconnected, or the SSID is empty or hidden. Never panic.
- Register `WlanRegisterNotification` (source `WLAN_NOTIFICATION_SOURCE_ACM`) for `connection_complete`, `disconnected`, and `interface_arrival/removal`. The callback only posts a message to the UI thread (nwg `Notice`). Do no work in the callback.
- Debounce events: wait 500 ms after the last event, then re-query the SSID once. Also run once at startup.
- Note: Windows 11 24H2 may require Location permission to read the SSID. If a Wi-Fi adapter is connected but the SSID comes back empty, show "SSID unavailable (check Location permission)" in status and log it.
- Network list for the UI: `WlanGetProfileList` (saved) ∪ `WlanGetAvailableNetworkList` (visible), deduplicated and sorted. The SSID field is an editable combo box, so a manual SSID can be typed.

### 5.2 Decision logic (US-03/04/05/09) — `logic.rs`
```
on_network_change(ssid):
  if !config.enabled: return                      // do NOT touch proxy at all
  if ssid matches rule:        apply(rule.proxy)
  else match config.unmatched:
       "disable": clear()
       "keep":    nothing
       "default": apply(config.default_proxy)     // if empty, clear()
```
- `ssid == None` (disconnected) counts as unmatched.
- Skip the registry write if the desired state equals the current state (avoids redundant notifications).
- Rule edit/delete/add, and toggling the master switch to ON, re-evaluate immediately against the current SSID.
- Manual Apply/Clear (US-07) persists until the next network-change event; it works even when auto-switch is disabled.

### 5.3 Applying the proxy (US-03/07, US-18) — `proxy.rs`
- Key: `HKCU\Software\Microsoft\Windows\CurrentVersion\Internet Settings`
- Apply: `ProxyServer` (REG_SZ) = `host:port`, `ProxyEnable` (REG_DWORD) = 1.
- Clear: `ProxyEnable` = 0 (leave `ProxyServer` as is).
- Then notify the system: `InternetSetOptionW(None, INTERNET_OPTION_SETTINGS_CHANGED /*39*/, None, 0)` then `INTERNET_OPTION_REFRESH /*37*/`.
- Return `Result`; on failure, log it and show it in the UI status.

### 5.4 Validation (US-02)
Proxy must match `host:port`:
- host = hostname (`[A-Za-z0-9.-]+`), IPv4, or `[IPv6]`.
- port = integer 1–65535.
- Reject: empty, missing port, spaces, a scheme (`http://`), invalid characters.
- Show an inline error and don't save.
- SSID must be non-empty (trimmed, max 32 bytes) and unique when adding; reject duplicates ("edit the existing rule").

## 6. UI

### 6.1 Main window (fixed size, not resizable)
- **Status panel (US-14):** Current SSID (or "Not connected"), Applied proxy (or "None"), Auto-switch ON/OFF, Last switch time (HH:MM:SS). Updates in real time on every network event.
- **Master toggle (US-06):** prominent checkbox/switch "Auto-switch enabled".
- **Rules list (US-08):** ListView with columns SSID | Proxy. Buttons: Add, Edit, Delete. Edit and Add use the same form: SSID combo box (US-02) + proxy text box + Save.
- **Unmatched behavior (US-09):** radio group: Disable / Keep current / Use default proxy. The default proxy text box is visible only for the third option.
- **Manual (US-07):** buttons "Apply Now" (applies the rule for the current SSID, else the unmatched behavior) and "Clear Proxy".
- **Options:** checkboxes "Start with Windows" (US-12), "Show notifications" (US-15). Button "View Log" (US-16) opens `app.log` in the default editor (`ShellExecuteW "open"`) or a read-only text window showing the last ~200 lines.
- All settings persist to `config.json` immediately on change.

### 6.2 Close, tray, menu (US-10/11)
- Window X hides the window; the process keeps running.
- Left-click tray icon: show and focus the window.
- Right-click tray menu: **Open Settings**, **Auto-Switch** (checkmark = enabled), **Apply Proxy Now**, **Clear Proxy**, **Quit**.
- Quit is the only way to exit: unregister notifications, close the handle, exit cleanly.
- The tray icon and tooltip change with the enabled/disabled state, and the tooltip shows the SSID + proxy.
- Notifications (US-15): a tray balloon like `Proxy → 10.0.0.1:8080 (abc)` or `Proxy disabled`. Shown only if `notify` is true and the proxy actually changed.

## 7. Lifecycle
- **Single instance:** named mutex; a second launch exits (optionally brings the first to front).
- **Start with Windows (US-12):** write the value `WifiProxySwitcher` = `"<exe path>" --hidden` in `HKCU\Software\Microsoft\Windows\CurrentVersion\Run`. Unchecking deletes the value. On launch, read the real registry state to set the checkbox.
- `--hidden`: start with the window never shown (no flash), tray only.
- On startup: load config → create tray → read the SSID → run `on_network_change`.
- No console window at launch, at boot, or during operation (no spawned `netsh`/`cmd`; use the native APIs only).

## 8. Logging (US-16) — `log.rs`
- Append lines `YYYY-MM-DD HH:MM:SS [LEVEL] msg` to `app.log`.
- Log: SSID changes, rule matched/unmatched, proxy applied/cleared/failed, config load errors.
- Trim: when the file exceeds 512 KB, rename it to `app.log.old` (replacing the old one) and start a new file.
- Don't log passwords or secrets (there are none).

## 9. Implementation Order
1. Scaffold the crate, `Cargo.toml`, `build.rs`, release profile, `windows_subsystem`.
2. `config.rs` + validation + unit tests.
3. `proxy.rs` (apply/clear/read) + manual test.
4. `wifi.rs` (current SSID, lists, notifications, debounce).
5. `logic.rs` + unit tests for the decision table (see §10).
6. `log.rs`.
7. `ui.rs` (window, tray, menu, balloon) and wire everything together.
8. `autostart.rs`, single instance, `--hidden`.
9. Release build; run through the Definition of Done.

## 10. Tests (must pass `cargo test`)
- Validation: `1.2.3.4:80` ✓, `proxy.corp:8080` ✓, `[::1]:3128` ✓, `host` ✗, `host:0` ✗, `host:70000` ✗, `http://h:80` ✗, `a b:80` ✗, empty ✗.
- Decision table: (enabled/disabled) × (matched/unmatched/None) × (disable/keep/default, default empty/set) yields the right Action.
- Config round-trip, corrupt-file fallback, duplicate SSID rejected.

## 11. Definition of Done
| Story | Check |
|---|---|
| US-01 | SSID is detected at startup and on change; no adapter, disconnected, and hidden SSID don't crash |
| US-02 | Pick or type an SSID, enter a validated proxy, rule persists after restart |
| US-03 | Connecting to a matched SSID sets the proxy in ≤ ~2 s, with no app restart needed |
| US-04 | Disconnecting or joining an unknown SSID clears the proxy (default behavior) |
| US-05 | Unlimited independent rules, duplicates blocked |
| US-06 | Master toggle persists; when OFF no automatic changes occur; tray icon reflects state |
| US-07 | Apply Now / Clear Proxy in the UI and tray; persists until the next network event |
| US-08 | List, edit, delete; changes take effect immediately |
| US-09 | Disable / Keep / Default proxy setting persists |
| US-10 | X hides to tray; tray click reopens; Quit exits fully |
| US-11 | Tray menu has all five items and reflects state |
| US-12 | Start-with-Windows toggle writes and removes the HKCU Run value; launches hidden |
| US-13 | No console window ever appears |
| US-14 | Status panel is live-updating |
| US-15 | Balloon notification on change; can be disabled |
| US-16 | Log file with trimming; "View Log" works |
| US-18 | Only HKCU is used; runs with no UAC prompt |

## 12. Deliverables
- Full source, `Cargo.toml`, `build.rs`, and a short `README.md` (build, run, config location, known limits).
- `target/release/<name>.exe`.
- A final message listing what was done, the measured RAM and CPU, and any deviations from this spec.
