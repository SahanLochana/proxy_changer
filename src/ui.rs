use std::time::{Duration, Instant};

use serde_json::{json, Value};
use tao::{
    dpi::LogicalSize,
    event::{Event, StartCause, WindowEvent},
    event_loop::{ControlFlow, EventLoop, EventLoopBuilder},
    window::{Window, WindowBuilder},
};
use tray_icon::{
    menu::{CheckMenuItem, Menu, MenuEvent, MenuItem, PredefinedMenuItem},
    MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent,
};
use windows::core::{w, PCWSTR};
use windows::Win32::UI::Shell::{
    ShellExecuteW, Shell_NotifyIconW, NIF_INFO, NIM_MODIFY, NOTIFYICONDATAW, NOTIFY_ICON_DATA_FLAGS,
};
use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
use wry::WebViewBuilder;

use crate::autostart;
use crate::config::{validate_proxy, validate_ssid, Config, Rule, UnmatchedBehavior};
use crate::log::log_path;
use crate::logic::{decide, should_execute_action, Action};
use crate::proxy::{apply_proxy, clear_proxy, read_proxy, ProxyState};
use crate::wifi::{SsidStatus, WlanClient};
use crate::{log_error, log_info, log_warn};

const HTML_CONTENT: &str = include_str!("../assets/index.html");

#[derive(Debug)]
pub enum UserEvent {
    WlanNotify,
    IpcMessage(String),
    DebounceTick,
}

pub struct UiState {
    pub config: Config,
    pub wlan_client: Option<WlanClient>,
    pub last_switch_time: String,
    pub last_wlan_event: Option<Instant>,
}

const APP_LOGO_ICO: &[u8] = include_bytes!("../assets/app_logo.ico");
const ENABLE_ICO: &[u8] = include_bytes!("../assets/enable.ico");
const DISABLE_ICO: &[u8] = include_bytes!("../assets/disable.ico");

fn load_ico_32(ico_bytes: &[u8]) -> Option<(Vec<u8>, u32, u32)> {
    if ico_bytes.len() < 6 {
        return None;
    }
    let count = u16::from_le_bytes([ico_bytes[4], ico_bytes[5]]) as usize;
    let mut best_offset = None;
    let target_size = 32u32;
    for i in 0..count {
        let entry_off = 6 + i * 16;
        if entry_off + 16 > ico_bytes.len() {
            break;
        }
        let w = match ico_bytes[entry_off] {
            0 => 256u32,
            val => val as u32,
        };
        let h = match ico_bytes[entry_off + 1] {
            0 => 256u32,
            val => val as u32,
        };
        let bpp = u16::from_le_bytes([ico_bytes[entry_off + 6], ico_bytes[entry_off + 7]]);
        let offset = u32::from_le_bytes([
            ico_bytes[entry_off + 12],
            ico_bytes[entry_off + 13],
            ico_bytes[entry_off + 14],
            ico_bytes[entry_off + 15],
        ]) as usize;
        if w == target_size && h == target_size && bpp == 32 {
            best_offset = Some(offset);
            break;
        }
    }

    let offset = best_offset?;
    if offset + 40 > ico_bytes.len() {
        return None;
    }
    let bi_size = u32::from_le_bytes([
        ico_bytes[offset],
        ico_bytes[offset + 1],
        ico_bytes[offset + 2],
        ico_bytes[offset + 3],
    ]) as usize;

    let pixels_start = offset + bi_size;
    let byte_count = (target_size * target_size * 4) as usize;
    if pixels_start + byte_count > ico_bytes.len() {
        return None;
    }

    let bgra = &ico_bytes[pixels_start..pixels_start + byte_count];
    let mut rgba = vec![0u8; byte_count];

    // ICO bitmap data is stored bottom-up BGRA. Convert to top-down RGBA.
    for y in 0..target_size {
        let src_row = y;
        let dst_row = (target_size - 1) - y;
        for x in 0..target_size {
            let si = ((src_row * target_size + x) * 4) as usize;
            let di = ((dst_row * target_size + x) * 4) as usize;
            let b = bgra[si];
            let g = bgra[si + 1];
            let r = bgra[si + 2];
            let a = bgra[si + 3];
            rgba[di] = r;
            rgba[di + 1] = g;
            rgba[di + 2] = b;
            rgba[di + 3] = a;
        }
    }

    Some((rgba, target_size, target_size))
}

fn load_tray_icon(ico_bytes: &[u8]) -> tray_icon::Icon {
    if let Some((rgba, w, h)) = load_ico_32(ico_bytes) {
        if let Ok(icon) = tray_icon::Icon::from_rgba(rgba, w, h) {
            return icon;
        }
    }
    tray_icon::Icon::from_rgba(vec![0; 32 * 32 * 4], 32, 32).expect("Failed to create fallback tray icon")
}

fn load_window_icon(ico_bytes: &[u8]) -> Option<tao::window::Icon> {
    if let Some((rgba, w, h)) = load_ico_32(ico_bytes) {
        tao::window::Icon::from_rgba(rgba, w, h).ok()
    } else {
        None
    }
}

pub fn run(start_hidden: bool) -> Result<(), Box<dyn std::error::Error>> {
    let event_loop: EventLoop<UserEvent> = EventLoopBuilder::<UserEvent>::with_user_event().build();
    let proxy = event_loop.create_proxy();

    // 1. Build window
    let window_icon = load_window_icon(APP_LOGO_ICO);
    let mut window_builder = WindowBuilder::new()
        .with_title("Wi-Fi Proxy Switcher")
        .with_inner_size(LogicalSize::new(800.0, 500.0))
        .with_resizable(false)
        .with_decorations(false)
        .with_visible(!start_hidden);

    if let Some(icon) = window_icon {
        window_builder = window_builder.with_window_icon(Some(icon));
    }

    let window = window_builder.build(&event_loop)?;

    // 2. Tray Menu
    let tray_menu = Menu::new();
    let item_open = MenuItem::new("Open Settings", true, None);
    let item_auto = CheckMenuItem::new("Auto-Switch", true, true, None);
    let item_apply = MenuItem::new("Apply Proxy Now", true, None);
    let item_clear = MenuItem::new("Clear Proxy", true, None);
    let item_quit = MenuItem::new("Quit", true, None);

    let _ = tray_menu.append(&item_open);
    let _ = tray_menu.append(&item_auto);
    let _ = tray_menu.append(&PredefinedMenuItem::separator());
    let _ = tray_menu.append(&item_apply);
    let _ = tray_menu.append(&item_clear);
    let _ = tray_menu.append(&PredefinedMenuItem::separator());
    let _ = tray_menu.append(&item_quit);

    let id_open = item_open.id().clone();
    let id_auto = item_auto.id().clone();
    let id_apply = item_apply.id().clone();
    let id_clear = item_clear.id().clone();
    let id_quit = item_quit.id().clone();

    let icon_enabled = load_tray_icon(ENABLE_ICO);
    let icon_disabled = load_tray_icon(DISABLE_ICO);

    let mut tray_icon = TrayIconBuilder::new()
        .with_tooltip("Wi-Fi Proxy Switcher")
        .with_icon(icon_enabled.clone())
        .with_menu(Box::new(tray_menu))
        .build()?;

    // 3. Build WebView
    let proxy_ipc = proxy.clone();
    let webview = WebViewBuilder::new()
        .with_html(HTML_CONTENT)
        .with_ipc_handler(move |request| {
            let body = request.body().clone();
            let _ = proxy_ipc.send_event(UserEvent::IpcMessage(body));
        })
        .build(&window)?;

    // 4. App State
    let config = Config::load();
    item_auto.set_checked(config.enabled);

    let mut wlan_client = match WlanClient::new() {
        Ok(client) => Some(client),
        Err(err) => {
            log_warn!("WLAN client initialization failed: {}", err);
            None
        }
    };

    // Register WLAN notification callback
    if let Some(client) = &mut wlan_client {
        let proxy_wlan = proxy.clone();
        if let Err(e) = client.register_notification(move || {
            let _ = proxy_wlan.send_event(UserEvent::WlanNotify);
        }) {
            log_warn!("Failed to register WLAN notification: {}", e);
        }
    }

    let mut state = UiState {
        config,
        wlan_client,
        last_switch_time: "Never".to_string(),
        last_wlan_event: None,
    };

    // Run initial network evaluation
    evaluate_network_rules(&mut state, &webview, &mut tray_icon, &icon_enabled, &icon_disabled, &item_auto, false);

    // 5. Event Loop
    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::WaitUntil(
            std::time::Instant::now() + Duration::from_millis(100),
        );

        // Check Tray menu events
        if let Ok(menu_event) = MenuEvent::receiver().try_recv() {
            if menu_event.id == id_open {
                window.set_visible(true);
                window.set_focus();
            } else if menu_event.id == id_auto {
                let new_state = !state.config.enabled;
                state.config.enabled = new_state;
                let _ = state.config.save();
                item_auto.set_checked(new_state);
                evaluate_network_rules(&mut state, &webview, &mut tray_icon, &icon_enabled, &icon_disabled, &item_auto, true);
            } else if menu_event.id == id_apply {
                manual_apply(&mut state, &webview, &mut tray_icon, &icon_enabled, &icon_disabled, &item_auto);
            } else if menu_event.id == id_clear {
                manual_clear(&mut state, &webview, &mut tray_icon, &icon_enabled, &icon_disabled, &item_auto);
            } else if menu_event.id == id_quit {
                log_info!("Application quitting from tray");
                if let Some(client) = &mut state.wlan_client {
                    client.unregister_notification();
                }
                *control_flow = ControlFlow::Exit;
            }
        }

        // Check Tray icon clicks
        if let Ok(tray_event) = TrayIconEvent::receiver().try_recv() {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = tray_event
            {
                window.set_visible(true);
                window.set_focus();
            }
        }

        // Check Debounce timer
        if let Some(event_time) = state.last_wlan_event {
            if event_time.elapsed() >= Duration::from_millis(500) {
                state.last_wlan_event = None;
                evaluate_network_rules(&mut state, &webview, &mut tray_icon, &icon_enabled, &icon_disabled, &item_auto, true);
            }
        }

        match event {
            Event::NewEvents(StartCause::Init) => {
                push_state_to_webview(&state, &webview);
            }
            Event::UserEvent(UserEvent::WlanNotify) => {
                // Restart 500ms debounce
                state.last_wlan_event = Some(Instant::now());
            }
            Event::UserEvent(UserEvent::IpcMessage(msg)) => {
                handle_ipc_message(
                    &msg,
                    &window,
                    &webview,
                    &mut state,
                    &mut tray_icon,
                    &icon_enabled,
                    &icon_disabled,
                    &item_auto,
                );
            }
            Event::WindowEvent {
                event: WindowEvent::CloseRequested,
                ..
            } => {
                // Hide to tray on close
                window.set_visible(false);
            }
            _ => {}
        }
    });
}

fn handle_ipc_message(
    msg: &str,
    window: &Window,
    webview: &wry::WebView,
    state: &mut UiState,
    tray_icon: &mut TrayIcon,
    icon_enabled: &tray_icon::Icon,
    icon_disabled: &tray_icon::Icon,
    item_auto: &CheckMenuItem,
) {
    let parsed: Value = match serde_json::from_str(msg) {
        Ok(v) => v,
        Err(_) => return,
    };

    let cmd = match parsed.get("cmd").and_then(|c| c.as_str()) {
        Some(c) => c,
        None => return,
    };

    match cmd {
        "init" => {
            push_state_to_webview(state, webview);
            push_logs_to_webview(webview);
        }
        "drag_window" => {
            let _ = window.drag_window();
        }
        "minimize" => {
            window.set_minimized(true);
        }
        "close" => {
            window.set_visible(false);
        }
        "toggle_master" => {
            if let Some(val) = parsed.get("enabled").and_then(|b| b.as_bool()) {
                state.config.enabled = val;
                let _ = state.config.save();
                item_auto.set_checked(val);
                evaluate_network_rules(state, webview, tray_icon, icon_enabled, icon_disabled, item_auto, true);
            }
        }
        "save_rule" => {
            let ssid = parsed.get("ssid").and_then(|s| s.as_str()).unwrap_or("");
            let proxy_addr = parsed.get("proxy").and_then(|s| s.as_str()).unwrap_or("");
            let edit_idx = parsed.get("index").and_then(|i| i.as_u64()).map(|i| i as usize);

            let valid_ssid = match validate_ssid(ssid, &state.config.rules, edit_idx) {
                Ok(s) => s,
                Err(err) => {
                    let _ = webview.evaluate_script(&format!("showError(\"{}\");", err));
                    return;
                }
            };

            if let Err(err) = validate_proxy(proxy_addr) {
                let _ = webview.evaluate_script(&format!("showError(\"{}\");", err));
                return;
            }

            if let Some(idx) = edit_idx {
                if let Some(rule) = state.config.rules.get_mut(idx) {
                    rule.ssid = valid_ssid;
                    rule.proxy = proxy_addr.to_string();
                }
            } else {
                state.config.rules.push(Rule {
                    ssid: valid_ssid,
                    proxy: proxy_addr.to_string(),
                });
            }

            let _ = state.config.save();
            let _ = webview.evaluate_script("hideError(); if (window.clearRuleEditor) { window.clearRuleEditor(); }");
            evaluate_network_rules(state, webview, tray_icon, icon_enabled, icon_disabled, item_auto, true);
        }
        "delete_rule" => {
            if let Some(idx) = parsed.get("index").and_then(|i| i.as_u64()).map(|i| i as usize) {
                if idx < state.config.rules.len() {
                    state.config.rules.remove(idx);
                    let _ = state.config.save();
                    evaluate_network_rules(state, webview, tray_icon, icon_enabled, icon_disabled, item_auto, true);
                }
            }
        }
        "set_unmatched" => {
            let unmatched_str = parsed.get("unmatched").and_then(|s| s.as_str()).unwrap_or("disable");
            let default_proxy = parsed.get("default_proxy").and_then(|s| s.as_str()).unwrap_or("").to_string();

            state.config.unmatched = match unmatched_str {
                "keep" => UnmatchedBehavior::Keep,
                "default" => UnmatchedBehavior::Default,
                _ => UnmatchedBehavior::Disable,
            };
            state.config.default_proxy = default_proxy;
            let _ = state.config.save();
            evaluate_network_rules(state, webview, tray_icon, icon_enabled, icon_disabled, item_auto, true);
        }
        "apply_now" => {
            manual_apply(state, webview, tray_icon, icon_enabled, icon_disabled, item_auto);
        }
        "clear_proxy" => {
            manual_clear(state, webview, tray_icon, icon_enabled, icon_disabled, item_auto);
        }
        "get_logs" => {
            push_logs_to_webview(webview);
        }
        "clear_logs" => {
            if let Err(e) = crate::log::clear_logs() {
                log_error!("Failed to clear logs: {}", e);
            } else {
                log_info!("Logs cleared by user");
            }
            push_logs_to_webview(webview);
        }
        "open_log" | "open_log_file" => {
            let p = log_path();
            if !p.exists() {
                let _ = std::fs::File::create(&p);
            }
            let wide: Vec<u16> = p.to_string_lossy().encode_utf16().chain(std::iter::once(0)).collect();
            unsafe {
                let _ = ShellExecuteW(
                    None,
                    w!("open"),
                    PCWSTR(wide.as_ptr()),
                    None,
                    None,
                    SW_SHOWNORMAL,
                );
            }
        }
        "toggle_autostart" => {
            if let Some(val) = parsed.get("enabled").and_then(|b| b.as_bool()) {
                if let Err(e) = autostart::set_autostart(val) {
                    log_error!("Failed to set autostart: {}", e);
                } else {
                    state.config.start_with_windows = val;
                    let _ = state.config.save();
                }
                push_state_to_webview(state, webview);
            }
        }
        "toggle_notify" => {
            if let Some(val) = parsed.get("enabled").and_then(|b| b.as_bool()) {
                state.config.notify = val;
                let _ = state.config.save();
                push_state_to_webview(state, webview);
            }
        }
        _ => {}
    }
}

fn manual_apply(
    state: &mut UiState,
    webview: &wry::WebView,
    tray_icon: &mut TrayIcon,
    icon_enabled: &tray_icon::Icon,
    icon_disabled: &tray_icon::Icon,
    item_auto: &CheckMenuItem,
) {
    let current_ssid = state.wlan_client.as_ref().and_then(|c| c.current_ssid());
    let target_proxy = if let Some(ref ssid) = current_ssid {
        if let Some(rule) = state.config.rules.iter().find(|r| r.ssid == *ssid) {
            Some(rule.proxy.clone())
        } else if state.config.unmatched == UnmatchedBehavior::Default && !state.config.default_proxy.trim().is_empty() {
            Some(state.config.default_proxy.clone())
        } else {
            None
        }
    } else if state.config.unmatched == UnmatchedBehavior::Default && !state.config.default_proxy.trim().is_empty() {
        Some(state.config.default_proxy.clone())
    } else {
        None
    };

    if let Some(proxy_addr) = target_proxy {
        log_info!("Manual apply: {}", proxy_addr);
        if let Err(e) = apply_proxy(&proxy_addr) {
            log_error!("Manual apply failed: {}", e);
        } else {
            record_switch_time(state);
            show_balloon_notification("Wi-Fi Proxy Switcher", &format!("Proxy → {} (Manual)", proxy_addr), state.config.notify);
        }
    } else {
        log_info!("Manual apply: clearing proxy");
        if let Err(e) = clear_proxy() {
            log_error!("Manual clear failed: {}", e);
        } else {
            record_switch_time(state);
            show_balloon_notification("Wi-Fi Proxy Switcher", "Proxy disabled (Manual)", state.config.notify);
        }
    }

    update_tray_and_ui(state, webview, tray_icon, icon_enabled, icon_disabled, item_auto);
}

fn manual_clear(
    state: &mut UiState,
    webview: &wry::WebView,
    tray_icon: &mut TrayIcon,
    icon_enabled: &tray_icon::Icon,
    icon_disabled: &tray_icon::Icon,
    item_auto: &CheckMenuItem,
) {
    log_info!("Manual clear requested");
    if let Err(e) = clear_proxy() {
        log_error!("Manual clear failed: {}", e);
    } else {
        record_switch_time(state);
        show_balloon_notification("Wi-Fi Proxy Switcher", "Proxy disabled (Manual)", state.config.notify);
    }

    update_tray_and_ui(state, webview, tray_icon, icon_enabled, icon_disabled, item_auto);
}

fn evaluate_network_rules(
    state: &mut UiState,
    webview: &wry::WebView,
    tray_icon: &mut TrayIcon,
    icon_enabled: &tray_icon::Icon,
    icon_disabled: &tray_icon::Icon,
    item_auto: &CheckMenuItem,
    can_notify: bool,
) {
    let current_ssid = state.wlan_client.as_ref().and_then(|c| c.current_ssid());
    let action = decide(&state.config, current_ssid.as_deref());

    let current_proxy = read_proxy().unwrap_or(ProxyState {
        enabled: false,
        server: None,
    });

    if should_execute_action(&action, &current_proxy) {
        match &action {
            Action::Apply(server) => {
                log_info!("Applying proxy: {}", server);
                if let Err(e) = apply_proxy(server) {
                    log_error!("Failed to apply proxy: {}", e);
                } else {
                    record_switch_time(state);
                    let tag = current_ssid.as_deref().unwrap_or("unmatched");
                    if can_notify {
                        show_balloon_notification("Wi-Fi Proxy Switcher", &format!("Proxy → {} ({})", server, tag), state.config.notify);
                    }
                }
            }
            Action::Clear => {
                log_info!("Clearing proxy");
                if let Err(e) = clear_proxy() {
                    log_error!("Failed to clear proxy: {}", e);
                } else {
                    record_switch_time(state);
                    if can_notify {
                        show_balloon_notification("Wi-Fi Proxy Switcher", "Proxy disabled", state.config.notify);
                    }
                }
            }
            Action::Nothing => {}
        }
    }

    update_tray_and_ui(state, webview, tray_icon, icon_enabled, icon_disabled, item_auto);
}

fn record_switch_time(state: &mut UiState) {
    unsafe {
        let st = windows::Win32::System::SystemInformation::GetLocalTime();
        state.last_switch_time = format!("{:02}:{:02}:{:02}", st.wHour, st.wMinute, st.wSecond);
    }
}

fn update_tray_and_ui(
    state: &UiState,
    webview: &wry::WebView,
    tray_icon: &mut TrayIcon,
    icon_enabled: &tray_icon::Icon,
    icon_disabled: &tray_icon::Icon,
    item_auto: &CheckMenuItem,
) {
    // Update tray icon and tooltip
    let proxy_active = match read_proxy() {
        Ok(p) => p.enabled,
        _ => false,
    };

    if state.config.enabled || proxy_active {
        let _ = tray_icon.set_icon(Some(icon_enabled.clone()));
    } else {
        let _ = tray_icon.set_icon(Some(icon_disabled.clone()));
    }

    item_auto.set_checked(state.config.enabled);

    let ssid_str = state.wlan_client.as_ref()
        .and_then(|c| c.current_ssid())
        .unwrap_or_else(|| "Not connected".to_string());

    let proxy_str = match read_proxy() {
        Ok(p) if p.enabled => p.server.unwrap_or_else(|| "None".to_string()),
        _ => "None".to_string(),
    };

    let state_str = if state.config.enabled { "Auto: ON" } else { "Auto: OFF" };
    let tip = format!("{} | Wi-Fi: {} | Proxy: {}", state_str, ssid_str, proxy_str);
    let _ = tray_icon.set_tooltip(Some(tip));

    push_state_to_webview(state, webview);
    push_logs_to_webview(webview);
}

fn push_logs_to_webview(webview: &wry::WebView) {
    let entries = crate::log::read_logs();
    if let Ok(json_val) = serde_json::to_string(&entries) {
        let script = format!("if (window.__renderLogs) {{ window.__renderLogs({}); }}", json_val);
        let _ = webview.evaluate_script(&script);
    }
}

fn push_state_to_webview(state: &UiState, webview: &wry::WebView) {
    let current_ssid = state.wlan_client.as_ref().and_then(|c| c.current_ssid());
    let (ssid_status_str, current_ssid_val) = match state.wlan_client.as_ref().map(|c| c.get_ssid_status()) {
        Some(SsidStatus::Connected(s)) => ("Connected", Some(s)),
        Some(SsidStatus::Disconnected) => ("Not connected", None),
        Some(SsidStatus::NoAdapter) => ("No Wi-Fi adapter detected", None),
        Some(SsidStatus::PermissionRequired) => ("SSID unavailable (check Location permission)", None),
        None => ("No Wi-Fi service", None),
    };

    let proxy_state = read_proxy();
    let proxy_active = proxy_state.as_ref().map(|p| p.enabled).unwrap_or(false);
    let active_proxy_str = match proxy_state {
        Ok(p) if p.enabled => p.server.unwrap_or_else(|| "None".to_string()),
        _ => "None".to_string(),
    };

    let network_list = state.wlan_client.as_ref().map(|c| c.network_list()).unwrap_or_default();

    let json_val = json!({
        "enabled": state.config.enabled,
        "rules": state.config.rules,
        "unmatched": match state.config.unmatched {
            UnmatchedBehavior::Disable => "disable",
            UnmatchedBehavior::Keep => "keep",
            UnmatchedBehavior::Default => "default",
        },
        "default_proxy": state.config.default_proxy,
        "start_with_windows": autostart::is_autostart_enabled(),
        "notify": state.config.notify,
        "current_ssid": current_ssid_val.or(current_ssid),
        "ssid_status": ssid_status_str,
        "active_proxy": active_proxy_str,
        "is_proxy_active": proxy_active,
        "last_switch_time": state.last_switch_time,
        "network_list": network_list,
    });

    let script = format!("if (window.__renderState) {{ window.__renderState({}); }}", json_val);
    let _ = webview.evaluate_script(&script);
}

fn show_balloon_notification(title: &str, text: &str, notify: bool) {
    if !notify {
        return;
    }
    unsafe {
        let mut nid = NOTIFYICONDATAW::default();
        nid.cbSize = std::mem::size_of::<NOTIFYICONDATAW>() as u32;
        nid.uFlags = NOTIFY_ICON_DATA_FLAGS(NIF_INFO.0);

        let title_w: Vec<u16> = title.encode_utf16().chain(std::iter::once(0)).collect();
        let text_w: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();

        for (i, &c) in title_w.iter().take(63).enumerate() {
            nid.szInfoTitle[i] = c;
        }
        for (i, &c) in text_w.iter().take(255).enumerate() {
            nid.szInfo[i] = c;
        }

        let _ = Shell_NotifyIconW(NIM_MODIFY, &nid);
    }
}
