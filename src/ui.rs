use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use native_windows_gui as nwg;
use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{HWND, POINT};
use windows::Win32::UI::Shell::ShellExecuteW;
use windows::Win32::UI::WindowsAndMessaging::{
    GetCursorPos, SetForegroundWindow, ShowWindow, SW_RESTORE, SW_SHOWNORMAL,
};

use crate::autostart;
use crate::config::{validate_proxy, validate_ssid, Config, Rule, UnmatchedBehavior};
use crate::log::log_path;
use crate::logic::{decide, should_execute_action, Action};
use crate::proxy::{apply_proxy, clear_proxy, read_proxy, ProxyState};
use crate::wifi::{SsidStatus, WlanClient};
use crate::{log_error, log_info, log_warn};

const ICON_ENABLED_BYTES: &[u8] = include_bytes!("../assets/icon_enabled.ico");
const ICON_DISABLED_BYTES: &[u8] = include_bytes!("../assets/icon_disabled.ico");

pub struct AppUi {
    pub window: nwg::Window,
    pub font: nwg::Font,
    pub bold_font: nwg::Font,
    pub title_font: nwg::Font,

    // Status Panel
    pub status_group: nwg::Label,
    pub lbl_ssid_title: nwg::Label,
    pub lbl_ssid_val: nwg::Label,
    pub lbl_proxy_title: nwg::Label,
    pub lbl_proxy_val: nwg::Label,
    pub lbl_state_title: nwg::Label,
    pub lbl_state_val: nwg::Label,
    pub lbl_time_title: nwg::Label,
    pub lbl_time_val: nwg::Label,

    // Master Switch
    pub chk_master_enable: nwg::CheckBox,

    // Rules
    pub lbl_rules_title: nwg::Label,
    pub list_rules: nwg::ListView,
    pub btn_add_rule: nwg::Button,
    pub btn_edit_rule: nwg::Button,
    pub btn_delete_rule: nwg::Button,

    // Add / Edit Rule Form
    pub lbl_form_title: nwg::Label,
    pub lbl_form_ssid: nwg::Label,
    pub cb_form_ssid: nwg::ComboBox<String>,
    pub txt_form_ssid: nwg::TextInput,
    pub lbl_form_proxy: nwg::Label,
    pub txt_form_proxy: nwg::TextInput,
    pub btn_form_save: nwg::Button,
    pub btn_form_cancel: nwg::Button,
    pub lbl_form_error: nwg::Label,

    // Unmatched Behavior
    pub lbl_unmatched_title: nwg::Label,
    pub rad_unmatched_disable: nwg::RadioButton,
    pub rad_unmatched_keep: nwg::RadioButton,
    pub rad_unmatched_default: nwg::RadioButton,
    pub txt_default_proxy: nwg::TextInput,

    // Manual Controls
    pub btn_apply_now: nwg::Button,
    pub btn_clear_proxy: nwg::Button,

    // Options
    pub chk_autostart: nwg::CheckBox,
    pub chk_notify: nwg::CheckBox,
    pub btn_view_log: nwg::Button,

    // Tray & Menus
    pub tray: nwg::TrayNotification,
    pub tray_menu: nwg::Menu,
    pub tray_item_open: nwg::MenuItem,
    pub tray_item_toggle: nwg::MenuItem,
    pub tray_item_sep1: nwg::MenuSeparator,
    pub tray_item_apply: nwg::MenuItem,
    pub tray_item_clear: nwg::MenuItem,
    pub tray_item_sep2: nwg::MenuSeparator,
    pub tray_item_quit: nwg::MenuItem,

    pub icon_enabled: nwg::Icon,
    pub icon_disabled: nwg::Icon,

    // Debounce & WLAN notice
    pub notice: nwg::Notice,
    pub debounce_timer: nwg::AnimationTimer,

    // App state
    pub config: RefCell<Config>,
    pub wlan_client: Option<WlanClient>,
    pub last_switch_time: RefCell<String>,
    pub editing_rule_index: RefCell<Option<usize>>,
    pub is_form_active: RefCell<bool>,
}

impl AppUi {
    pub fn build(start_hidden: bool) -> Result<Rc<Self>, String> {
        let icon_enabled = nwg::Icon::from_bin(ICON_ENABLED_BYTES)
            .map_err(|e| format!("Failed to load enabled icon: {:?}", e))?;
        let icon_disabled = nwg::Icon::from_bin(ICON_DISABLED_BYTES)
            .map_err(|e| format!("Failed to load disabled icon: {:?}", e))?;

        let mut font = nwg::Font::default();
        nwg::Font::builder()
            .size(15)
            .family("Segoe UI")
            .build(&mut font)
            .map_err(|e| format!("Font build error: {:?}", e))?;

        let mut bold_font = nwg::Font::default();
        nwg::Font::builder()
            .size(15)
            .weight(700)
            .family("Segoe UI")
            .build(&mut bold_font)
            .map_err(|e| format!("Font build error: {:?}", e))?;

        let mut title_font = nwg::Font::default();
        nwg::Font::builder()
            .size(17)
            .weight(700)
            .family("Segoe UI")
            .build(&mut title_font)
            .map_err(|e| format!("Font build error: {:?}", e))?;

        let mut window = nwg::Window::default();
        nwg::Window::builder()
            .size((520, 680))
            .position((300, 150))
            .title("Wi-Fi Proxy Auto-Switcher")
            .flags(nwg::WindowFlags::WINDOW | nwg::WindowFlags::MINIMIZE_BOX)
            .build(&mut window)
            .map_err(|e| format!("Window build error: {:?}", e))?;

        // Status Panel (Y: 10 .. 100)
        let mut status_group = nwg::Label::default();
        nwg::Label::builder()
            .text("System Status")
            .position((15, 10))
            .size((490, 20))
            .font(Some(&title_font))
            .parent(&window)
            .build(&mut status_group)
            .unwrap();

        let mut lbl_ssid_title = nwg::Label::default();
        nwg::Label::builder()
            .text("Current Wi-Fi:")
            .position((20, 35))
            .size((110, 20))
            .font(Some(&bold_font))
            .parent(&window)
            .build(&mut lbl_ssid_title)
            .unwrap();

        let mut lbl_ssid_val = nwg::Label::default();
        nwg::Label::builder()
            .text("Checking...")
            .position((135, 35))
            .size((360, 20))
            .font(Some(&font))
            .parent(&window)
            .build(&mut lbl_ssid_val)
            .unwrap();

        let mut lbl_proxy_title = nwg::Label::default();
        nwg::Label::builder()
            .text("Active Proxy:")
            .position((20, 58))
            .size((110, 20))
            .font(Some(&bold_font))
            .parent(&window)
            .build(&mut lbl_proxy_title)
            .unwrap();

        let mut lbl_proxy_val = nwg::Label::default();
        nwg::Label::builder()
            .text("Checking...")
            .position((135, 58))
            .size((360, 20))
            .font(Some(&font))
            .parent(&window)
            .build(&mut lbl_proxy_val)
            .unwrap();

        let mut lbl_state_title = nwg::Label::default();
        nwg::Label::builder()
            .text("Auto-Switch:")
            .position((20, 81))
            .size((110, 20))
            .font(Some(&bold_font))
            .parent(&window)
            .build(&mut lbl_state_title)
            .unwrap();

        let mut lbl_state_val = nwg::Label::default();
        nwg::Label::builder()
            .text("Enabled")
            .position((135, 81))
            .size((130, 20))
            .font(Some(&font))
            .parent(&window)
            .build(&mut lbl_state_val)
            .unwrap();

        let mut lbl_time_title = nwg::Label::default();
        nwg::Label::builder()
            .text("Last Switch:")
            .position((270, 81))
            .size((90, 20))
            .font(Some(&bold_font))
            .parent(&window)
            .build(&mut lbl_time_title)
            .unwrap();

        let mut lbl_time_val = nwg::Label::default();
        nwg::Label::builder()
            .text("Never")
            .position((365, 81))
            .size((130, 20))
            .font(Some(&font))
            .parent(&window)
            .build(&mut lbl_time_val)
            .unwrap();

        // Master Switch (Y: 110)
        let mut chk_master_enable = nwg::CheckBox::default();
        nwg::CheckBox::builder()
            .text("Enable automatic proxy switching based on Wi-Fi SSID")
            .position((20, 110))
            .size((480, 25))
            .font(Some(&bold_font))
            .parent(&window)
            .build(&mut chk_master_enable)
            .unwrap();

        // Rules List (Y: 145 .. 275)
        let mut lbl_rules_title = nwg::Label::default();
        nwg::Label::builder()
            .text("SSID Proxy Rules")
            .position((15, 145))
            .size((490, 20))
            .font(Some(&title_font))
            .parent(&window)
            .build(&mut lbl_rules_title)
            .unwrap();

        let mut list_rules = nwg::ListView::default();
        nwg::ListView::builder()
            .position((20, 170))
            .size((475, 105))
            .parent(&window)
            .build(&mut list_rules)
            .unwrap();

        list_rules.insert_column("Wi-Fi SSID");
        list_rules.insert_column("Proxy (host:port)");
        list_rules.set_headers_enabled(true);

        let mut btn_add_rule = nwg::Button::default();
        nwg::Button::builder()
            .text("Add Rule")
            .position((20, 282))
            .size((90, 28))
            .font(Some(&font))
            .parent(&window)
            .build(&mut btn_add_rule)
            .unwrap();

        let mut btn_edit_rule = nwg::Button::default();
        nwg::Button::builder()
            .text("Edit Rule")
            .position((118, 282))
            .size((90, 28))
            .font(Some(&font))
            .parent(&window)
            .build(&mut btn_edit_rule)
            .unwrap();

        let mut btn_delete_rule = nwg::Button::default();
        nwg::Button::builder()
            .text("Delete Rule")
            .position((216, 282))
            .size((90, 28))
            .font(Some(&font))
            .parent(&window)
            .build(&mut btn_delete_rule)
            .unwrap();

        // Add / Edit Form (Y: 318 .. 425)
        let mut lbl_form_title = nwg::Label::default();
        nwg::Label::builder()
            .text("Add / Edit Rule:")
            .position((20, 318))
            .size((200, 20))
            .font(Some(&bold_font))
            .parent(&window)
            .build(&mut lbl_form_title)
            .unwrap();

        let mut lbl_form_ssid = nwg::Label::default();
        nwg::Label::builder()
            .text("Pick SSID:")
            .position((20, 342))
            .size((75, 20))
            .font(Some(&font))
            .parent(&window)
            .build(&mut lbl_form_ssid)
            .unwrap();

        let mut cb_form_ssid = nwg::ComboBox::default();
        nwg::ComboBox::builder()
            .position((100, 340))
            .size((185, 120))
            .font(Some(&font))
            .parent(&window)
            .build(&mut cb_form_ssid)
            .unwrap();

        let mut txt_form_ssid = nwg::TextInput::default();
        nwg::TextInput::builder()
            .position((292, 340))
            .size((203, 25))
            .placeholder_text(Some("Or type SSID manually"))
            .font(Some(&font))
            .parent(&window)
            .build(&mut txt_form_ssid)
            .unwrap();

        let mut lbl_form_proxy = nwg::Label::default();
        nwg::Label::builder()
            .text("Proxy:")
            .position((20, 372))
            .size((75, 20))
            .font(Some(&font))
            .parent(&window)
            .build(&mut lbl_form_proxy)
            .unwrap();

        let mut txt_form_proxy = nwg::TextInput::default();
        nwg::TextInput::builder()
            .position((100, 370))
            .size((230, 25))
            .placeholder_text(Some("e.g. 10.0.0.1:8080 or proxy.corp:8080"))
            .font(Some(&font))
            .parent(&window)
            .build(&mut txt_form_proxy)
            .unwrap();

        let mut btn_form_save = nwg::Button::default();
        nwg::Button::builder()
            .text("Save")
            .position((338, 369))
            .size((75, 27))
            .font(Some(&bold_font))
            .parent(&window)
            .build(&mut btn_form_save)
            .unwrap();

        let mut btn_form_cancel = nwg::Button::default();
        nwg::Button::builder()
            .text("Cancel")
            .position((420, 369))
            .size((75, 27))
            .font(Some(&font))
            .parent(&window)
            .build(&mut btn_form_cancel)
            .unwrap();

        let mut lbl_form_error = nwg::Label::default();
        nwg::Label::builder()
            .text("")
            .position((20, 400))
            .size((475, 20))
            .font(Some(&font))
            .parent(&window)
            .build(&mut lbl_form_error)
            .unwrap();

        // Unmatched Behavior (Y: 425 .. 505)
        let mut lbl_unmatched_title = nwg::Label::default();
        nwg::Label::builder()
            .text("When connected to unmatched Wi-Fi / disconnected:")
            .position((15, 425))
            .size((490, 20))
            .font(Some(&bold_font))
            .parent(&window)
            .build(&mut lbl_unmatched_title)
            .unwrap();

        let mut rad_unmatched_disable = nwg::RadioButton::default();
        nwg::RadioButton::builder()
            .text("Disable proxy (default)")
            .position((20, 448))
            .size((160, 22))
            .font(Some(&font))
            .parent(&window)
            .build(&mut rad_unmatched_disable)
            .unwrap();

        let mut rad_unmatched_keep = nwg::RadioButton::default();
        nwg::RadioButton::builder()
            .text("Keep current proxy")
            .position((185, 448))
            .size((140, 22))
            .font(Some(&font))
            .parent(&window)
            .build(&mut rad_unmatched_keep)
            .unwrap();

        let mut rad_unmatched_default = nwg::RadioButton::default();
        nwg::RadioButton::builder()
            .text("Use default proxy:")
            .position((20, 473))
            .size((140, 22))
            .font(Some(&font))
            .parent(&window)
            .build(&mut rad_unmatched_default)
            .unwrap();

        let mut txt_default_proxy = nwg::TextInput::default();
        nwg::TextInput::builder()
            .position((165, 472))
            .size((330, 25))
            .placeholder_text(Some("host:port (e.g. 192.168.1.1:8080)"))
            .font(Some(&font))
            .parent(&window)
            .build(&mut txt_default_proxy)
            .unwrap();

        // Manual Controls (Y: 512)
        let mut btn_apply_now = nwg::Button::default();
        nwg::Button::builder()
            .text("Apply Proxy Now")
            .position((20, 510))
            .size((145, 30))
            .font(Some(&font))
            .parent(&window)
            .build(&mut btn_apply_now)
            .unwrap();

        let mut btn_clear_proxy = nwg::Button::default();
        nwg::Button::builder()
            .text("Clear Proxy")
            .position((175, 510))
            .size((145, 30))
            .font(Some(&font))
            .parent(&window)
            .build(&mut btn_clear_proxy)
            .unwrap();

        // Options (Y: 550 .. 630)
        let mut chk_autostart = nwg::CheckBox::default();
        nwg::CheckBox::builder()
            .text("Start with Windows (minimized to tray)")
            .position((20, 552))
            .size((280, 22))
            .font(Some(&font))
            .parent(&window)
            .build(&mut chk_autostart)
            .unwrap();

        let mut chk_notify = nwg::CheckBox::default();
        nwg::CheckBox::builder()
            .text("Show notification balloons on proxy changes")
            .position((20, 577))
            .size((320, 22))
            .font(Some(&font))
            .parent(&window)
            .build(&mut chk_notify)
            .unwrap();

        let mut btn_view_log = nwg::Button::default();
        nwg::Button::builder()
            .text("View Log")
            .position((380, 560))
            .size((115, 32))
            .font(Some(&font))
            .parent(&window)
            .build(&mut btn_view_log)
            .unwrap();

        // Tray Notification & Context Menu
        let mut tray = nwg::TrayNotification::default();
        nwg::TrayNotification::builder()
            .parent(&window)
            .icon(Some(&icon_enabled))
            .tip(Some("Wi-Fi Proxy Auto-Switcher"))
            .build(&mut tray)
            .map_err(|e| format!("Tray build error: {:?}", e))?;

        let mut tray_menu = nwg::Menu::default();
        nwg::Menu::builder()
            .popup(true)
            .parent(&window)
            .build(&mut tray_menu)
            .map_err(|e| format!("Menu build error: {:?}", e))?;

        let mut tray_item_open = nwg::MenuItem::default();
        nwg::MenuItem::builder()
            .text("Open Settings")
            .parent(&tray_menu)
            .build(&mut tray_item_open)
            .unwrap();

        let mut tray_item_toggle = nwg::MenuItem::default();
        nwg::MenuItem::builder()
            .text("Auto-Switch")
            .parent(&tray_menu)
            .build(&mut tray_item_toggle)
            .unwrap();

        let mut tray_item_sep1 = nwg::MenuSeparator::default();
        nwg::MenuSeparator::builder()
            .parent(&tray_menu)
            .build(&mut tray_item_sep1)
            .unwrap();

        let mut tray_item_apply = nwg::MenuItem::default();
        nwg::MenuItem::builder()
            .text("Apply Proxy Now")
            .parent(&tray_menu)
            .build(&mut tray_item_apply)
            .unwrap();

        let mut tray_item_clear = nwg::MenuItem::default();
        nwg::MenuItem::builder()
            .text("Clear Proxy")
            .parent(&tray_menu)
            .build(&mut tray_item_clear)
            .unwrap();

        let mut tray_item_sep2 = nwg::MenuSeparator::default();
        nwg::MenuSeparator::builder()
            .parent(&tray_menu)
            .build(&mut tray_item_sep2)
            .unwrap();

        let mut tray_item_quit = nwg::MenuItem::default();
        nwg::MenuItem::builder()
            .text("Quit")
            .parent(&tray_menu)
            .build(&mut tray_item_quit)
            .unwrap();

        // Debounce & Notice
        let mut notice = nwg::Notice::default();
        nwg::Notice::builder()
            .parent(&window)
            .build(&mut notice)
            .map_err(|e| format!("Notice build error: {:?}", e))?;

        let mut debounce_timer = nwg::AnimationTimer::default();
        nwg::AnimationTimer::builder()
            .interval(Duration::from_millis(500))
            .max_tick(Some(1))
            .active(false)
            .parent(&window)
            .build(&mut debounce_timer)
            .map_err(|e| format!("Timer build error: {:?}", e))?;

        let wlan_client = match WlanClient::new() {
            Ok(client) => Some(client),
            Err(err) => {
                log_warn!("WLAN client initialization failed: {}", err);
                None
            }
        };

        let config = Config::load();

        let ui = Rc::new(AppUi {
            window,
            font,
            bold_font,
            title_font,
            status_group,
            lbl_ssid_title,
            lbl_ssid_val,
            lbl_proxy_title,
            lbl_proxy_val,
            lbl_state_title,
            lbl_state_val,
            lbl_time_title,
            lbl_time_val,
            chk_master_enable,
            lbl_rules_title,
            list_rules,
            btn_add_rule,
            btn_edit_rule,
            btn_delete_rule,
            lbl_form_title,
            lbl_form_ssid,
            cb_form_ssid,
            txt_form_ssid,
            lbl_form_proxy,
            txt_form_proxy,
            btn_form_save,
            btn_form_cancel,
            lbl_form_error,
            lbl_unmatched_title,
            rad_unmatched_disable,
            rad_unmatched_keep,
            rad_unmatched_default,
            txt_default_proxy,
            btn_apply_now,
            btn_clear_proxy,
            chk_autostart,
            chk_notify,
            btn_view_log,
            tray,
            tray_menu,
            tray_item_open,
            tray_item_toggle,
            tray_item_sep1,
            tray_item_apply,
            tray_item_clear,
            tray_item_sep2,
            tray_item_quit,
            icon_enabled,
            icon_disabled,
            notice,
            debounce_timer,
            config: RefCell::new(config),
            wlan_client,
            last_switch_time: RefCell::new("Never".to_string()),
            editing_rule_index: RefCell::new(None),
            is_form_active: RefCell::new(false),
        });

        // Set ListView column widths
        ui.list_rules.set_column_width(0, 200);
        ui.list_rules.set_column_width(1, 260);

        // Bind events
        Self::bind_events(&ui);

        // Populate controls from config
        ui.sync_ui_from_config();

        // Refresh discovered networks in form combo
        ui.refresh_network_list();

        // Check registry for autostart
        ui.chk_autostart.set_check_state(if autostart::is_autostart_enabled() {
            nwg::CheckBoxState::Checked
        } else {
            nwg::CheckBoxState::Unchecked
        });

        // Register WLAN notification callback
        if let Some(client) = &ui.wlan_client {
            let sender_ptr = Box::into_raw(Box::new(ui.notice.sender())) as *const std::ffi::c_void;
            if let Err(e) = client.register_notification(sender_ptr) {
                log_warn!("Failed to register WLAN notification: {}", e);
            }
        }

        // Run initial network evaluation
        ui.on_network_change(false);

        // Show window if not started with --hidden
        if !start_hidden {
            ui.window.set_visible(true);
        }

        Ok(ui)
    }

    fn bind_events(ui: &Rc<AppUi>) {
        let ui_handle = ui.window.handle;
        let ui_c = Rc::clone(ui);

        nwg::bind_event_handler(&ui_handle, &ui_handle, move |evt, _evt_data, handle| {
            match evt {
                nwg::Event::OnWindowClose => {
                    if handle == ui_c.window.handle {
                        // X hides the window; keeps running
                        ui_c.window.set_visible(false);
                    }
                }
                nwg::Event::OnContextMenu => {
                    if handle == ui_c.tray.handle {
                        // Right-click on tray: show context menu at cursor
                        unsafe {
                            let mut pt = POINT::default();
                            let _ = GetCursorPos(&mut pt);
                            ui_c.tray_item_toggle.set_checked(ui_c.config.borrow().enabled);
                            ui_c.tray_menu.popup(pt.x, pt.y);
                        }
                    }
                }
                nwg::Event::OnMousePress(nwg::MousePressEvent::MousePressLeftUp) => {
                    if handle == ui_c.tray.handle {
                        ui_c.show_and_focus();
                    }
                }
                nwg::Event::OnMenuItemSelected => {
                    if handle == ui_c.tray_item_open.handle {
                        ui_c.show_and_focus();
                    } else if handle == ui_c.tray_item_toggle.handle {
                        let new_state = !ui_c.config.borrow().enabled;
                        ui_c.set_master_enabled(new_state);
                    } else if handle == ui_c.tray_item_apply.handle {
                        ui_c.manual_apply();
                    } else if handle == ui_c.tray_item_clear.handle {
                        ui_c.manual_clear();
                    } else if handle == ui_c.tray_item_quit.handle {
                        ui_c.quit();
                    }
                }
                nwg::Event::OnButtonClick => {
                    if handle == ui_c.chk_master_enable.handle {
                        let new_state = ui_c.chk_master_enable.check_state() == nwg::CheckBoxState::Checked;
                        ui_c.set_master_enabled(new_state);
                    } else if handle == ui_c.btn_add_rule.handle {
                        ui_c.start_add_rule();
                    } else if handle == ui_c.btn_edit_rule.handle {
                        ui_c.start_edit_rule();
                    } else if handle == ui_c.btn_delete_rule.handle {
                        ui_c.delete_selected_rule();
                    } else if handle == ui_c.btn_form_save.handle {
                        ui_c.save_rule_form();
                    } else if handle == ui_c.btn_form_cancel.handle {
                        ui_c.cancel_rule_form();
                    } else if handle == ui_c.rad_unmatched_disable.handle
                        || handle == ui_c.rad_unmatched_keep.handle
                        || handle == ui_c.rad_unmatched_default.handle
                    {
                        ui_c.update_unmatched_behavior();
                    } else if handle == ui_c.btn_apply_now.handle {
                        ui_c.manual_apply();
                    } else if handle == ui_c.btn_clear_proxy.handle {
                        ui_c.manual_clear();
                    } else if handle == ui_c.chk_autostart.handle {
                        ui_c.toggle_autostart();
                    } else if handle == ui_c.chk_notify.handle {
                        let checked = ui_c.chk_notify.check_state() == nwg::CheckBoxState::Checked;
                        ui_c.config.borrow_mut().notify = checked;
                        let _ = ui_c.config.borrow().save();
                    } else if handle == ui_c.btn_view_log.handle {
                        ui_c.view_log();
                    }
                }
                nwg::Event::OnComboxBoxSelection => {
                    if handle == ui_c.cb_form_ssid.handle {
                        if let Some(selected) = ui_c.cb_form_ssid.selection_string() {
                            ui_c.txt_form_ssid.set_text(&selected);
                        }
                    }
                }
                nwg::Event::OnNotice => {
                    if handle == ui_c.notice.handle {
                        // WLAN notification posted -> restart debounce timer (500 ms)
                        ui_c.debounce_timer.start();
                    }
                }
                nwg::Event::OnTimerTick => {
                    if handle == ui_c.debounce_timer.handle {
                        ui_c.debounce_timer.stop();
                        ui_c.on_network_change(true);
                    }
                }
                _ => {}
            }
        });
    }

    pub fn show_and_focus(&self) {
        self.window.set_visible(true);
        unsafe {
            let hwnd = HWND(self.window.handle.hwnd().unwrap() as *mut _);
            let _ = ShowWindow(hwnd, SW_RESTORE);
            let _ = SetForegroundWindow(hwnd);
        }
        self.refresh_network_list();
    }

    pub fn set_master_enabled(&self, enabled: bool) {
        {
            let mut cfg = self.config.borrow_mut();
            cfg.enabled = enabled;
            let _ = cfg.save();
        }
        self.chk_master_enable.set_check_state(if enabled {
            nwg::CheckBoxState::Checked
        } else {
            nwg::CheckBoxState::Unchecked
        });
        self.tray_item_toggle.set_checked(enabled);
        self.update_tray_icon_and_tip();
        self.update_status_labels();

        if enabled {
            log_info!("Master switch turned ON, re-evaluating current network");
            self.on_network_change(true);
        } else {
            log_info!("Master switch turned OFF");
        }
    }

    pub fn sync_ui_from_config(&self) {
        let cfg = self.config.borrow();
        self.chk_master_enable.set_check_state(if cfg.enabled {
            nwg::CheckBoxState::Checked
        } else {
            nwg::CheckBoxState::Unchecked
        });
        self.tray_item_toggle.set_checked(cfg.enabled);
        self.chk_notify.set_check_state(if cfg.notify {
            nwg::CheckBoxState::Checked
        } else {
            nwg::CheckBoxState::Unchecked
        });
        self.txt_default_proxy.set_text(&cfg.default_proxy);

        match cfg.unmatched {
            UnmatchedBehavior::Disable => {
                self.rad_unmatched_disable.set_check_state(nwg::RadioButtonState::Checked);
                self.rad_unmatched_keep.set_check_state(nwg::RadioButtonState::Unchecked);
                self.rad_unmatched_default.set_check_state(nwg::RadioButtonState::Unchecked);
                self.txt_default_proxy.set_enabled(false);
            }
            UnmatchedBehavior::Keep => {
                self.rad_unmatched_disable.set_check_state(nwg::RadioButtonState::Unchecked);
                self.rad_unmatched_keep.set_check_state(nwg::RadioButtonState::Checked);
                self.rad_unmatched_default.set_check_state(nwg::RadioButtonState::Unchecked);
                self.txt_default_proxy.set_enabled(false);
            }
            UnmatchedBehavior::Default => {
                self.rad_unmatched_disable.set_check_state(nwg::RadioButtonState::Unchecked);
                self.rad_unmatched_keep.set_check_state(nwg::RadioButtonState::Unchecked);
                self.rad_unmatched_default.set_check_state(nwg::RadioButtonState::Checked);
                self.txt_default_proxy.set_enabled(true);
            }
        }

        self.refresh_rules_list();
        self.update_tray_icon_and_tip();
    }

    pub fn refresh_rules_list(&self) {
        self.list_rules.clear();
        let cfg = self.config.borrow();
        for (i, rule) in cfg.rules.iter().enumerate() {
            self.list_rules.insert_item(nwg::InsertListViewItem {
                index: Some(i as i32),
                column_index: 0,
                text: Some(rule.ssid.clone()),
                image: None,
            });
            self.list_rules.update_item(
                i,
                nwg::InsertListViewItem {
                    index: Some(i as i32),
                    column_index: 1,
                    text: Some(rule.proxy.clone()),
                    image: None,
                },
            );
        }
    }

    pub fn refresh_network_list(&self) {
        if let Some(client) = &self.wlan_client {
            let list = client.network_list();
            self.cb_form_ssid.set_collection(list);
        }
    }

    pub fn start_add_rule(&self) {
        *self.editing_rule_index.borrow_mut() = None;
        *self.is_form_active.borrow_mut() = true;
        self.lbl_form_title.set_text("Add New Rule:");
        self.txt_form_ssid.set_text("");
        self.txt_form_proxy.set_text("");
        self.lbl_form_error.set_text("");
        self.refresh_network_list();
    }

    pub fn start_edit_rule(&self) {
        let selected = self.list_rules.selected_item();
        if let Some(idx) = selected {
            let cfg = self.config.borrow();
            if let Some(rule) = cfg.rules.get(idx) {
                *self.editing_rule_index.borrow_mut() = Some(idx);
                *self.is_form_active.borrow_mut() = true;
                self.lbl_form_title.set_text(&format!("Edit Rule #{}:", idx + 1));
                self.txt_form_ssid.set_text(&rule.ssid);
                self.txt_form_proxy.set_text(&rule.proxy);
                self.lbl_form_error.set_text("");
                self.cb_form_ssid.set_selection_string(&rule.ssid);
            }
        } else {
            self.lbl_form_error.set_text("Please select a rule from the list to edit.");
        }
    }

    pub fn cancel_rule_form(&self) {
        *self.editing_rule_index.borrow_mut() = None;
        *self.is_form_active.borrow_mut() = false;
        self.lbl_form_title.set_text("Add / Edit Rule:");
        self.txt_form_ssid.set_text("");
        self.txt_form_proxy.set_text("");
        self.lbl_form_error.set_text("");
    }

    pub fn save_rule_form(&self) {
        let ssid_raw = self.txt_form_ssid.text();
        let proxy_raw = self.txt_form_proxy.text();

        let editing_idx = *self.editing_rule_index.borrow();

        // 1. Validate SSID
        let valid_ssid = {
            let cfg = self.config.borrow();
            match validate_ssid(&ssid_raw, &cfg.rules, editing_idx) {
                Ok(s) => s,
                Err(err) => {
                    self.lbl_form_error.set_text(&format!("Error: {}", err));
                    return;
                }
            }
        };

        // 2. Validate Proxy
        let proxy_trimmed = proxy_raw.trim();
        if let Err(err) = validate_proxy(proxy_trimmed) {
            self.lbl_form_error.set_text(&format!("Error: {}", err));
            return;
        }

        // Save rule
        {
            let mut cfg = self.config.borrow_mut();
            if let Some(idx) = editing_idx {
                if let Some(rule) = cfg.rules.get_mut(idx) {
                    rule.ssid = valid_ssid;
                    rule.proxy = proxy_trimmed.to_string();
                }
            } else {
                cfg.rules.push(Rule {
                    ssid: valid_ssid,
                    proxy: proxy_trimmed.to_string(),
                });
            }
            let _ = cfg.save();
        }

        self.cancel_rule_form();
        self.refresh_rules_list();
        log_info!("Rule saved, re-evaluating network rules");
        self.on_network_change(true);
    }

    pub fn delete_selected_rule(&self) {
        let selected = self.list_rules.selected_item();
        if let Some(idx) = selected {
            {
                let mut cfg = self.config.borrow_mut();
                if idx < cfg.rules.len() {
                    let removed = cfg.rules.remove(idx);
                    log_info!("Deleted rule for SSID \"{}\"", removed.ssid);
                    let _ = cfg.save();
                }
            }
            self.cancel_rule_form();
            self.refresh_rules_list();
            self.on_network_change(true);
        } else {
            self.lbl_form_error.set_text("Please select a rule from the list to delete.");
        }
    }

    pub fn update_unmatched_behavior(&self) {
        let behavior = if self.rad_unmatched_disable.check_state() == nwg::RadioButtonState::Checked {
            self.txt_default_proxy.set_enabled(false);
            UnmatchedBehavior::Disable
        } else if self.rad_unmatched_keep.check_state() == nwg::RadioButtonState::Checked {
            self.txt_default_proxy.set_enabled(false);
            UnmatchedBehavior::Keep
        } else {
            self.txt_default_proxy.set_enabled(true);
            UnmatchedBehavior::Default
        };

        let default_proxy_val = self.txt_default_proxy.text().trim().to_string();

        {
            let mut cfg = self.config.borrow_mut();
            cfg.unmatched = behavior;
            cfg.default_proxy = default_proxy_val;
            let _ = cfg.save();
        }

        log_info!("Unmatched behavior updated to {:?}", behavior);
        self.on_network_change(true);
    }

    pub fn toggle_autostart(&self) {
        let target = self.chk_autostart.check_state() == nwg::CheckBoxState::Checked;
        if let Err(e) = autostart::set_autostart(target) {
            log_error!("Failed to update autostart setting: {}", e);
            self.lbl_form_error.set_text(&format!("Autostart error: {}", e));
        } else {
            let mut cfg = self.config.borrow_mut();
            cfg.start_with_windows = target;
            let _ = cfg.save();
            log_info!("Autostart set to {}", target);
        }
    }

    pub fn manual_apply(&self) {
        let (current_ssid, _) = self.query_current_network();
        let target_proxy = {
            let cfg = self.config.borrow();
            if let Some(ref ssid) = current_ssid {
                if let Some(rule) = cfg.rules.iter().find(|r| r.ssid == *ssid) {
                    Some(rule.proxy.clone())
                } else if cfg.unmatched == UnmatchedBehavior::Default && !cfg.default_proxy.trim().is_empty() {
                    Some(cfg.default_proxy.clone())
                } else {
                    None
                }
            } else if cfg.unmatched == UnmatchedBehavior::Default && !cfg.default_proxy.trim().is_empty() {
                Some(cfg.default_proxy.clone())
            } else {
                None
            }
        };

        if let Some(proxy_addr) = target_proxy {
            log_info!("Manual Apply: setting proxy to {}", proxy_addr);
            if let Err(e) = apply_proxy(&proxy_addr) {
                log_error!("Manual apply failed: {}", e);
                self.lbl_form_error.set_text(&format!("Failed to apply proxy: {}", e));
            } else {
                self.record_switch_time();
                self.show_balloon_notification(&format!("Proxy → {} (Manual)", proxy_addr));
            }
        } else {
            log_info!("Manual Apply: no matched rule or default proxy, clearing proxy");
            if let Err(e) = clear_proxy() {
                log_error!("Manual clear failed: {}", e);
            } else {
                self.record_switch_time();
                self.show_balloon_notification("Proxy disabled (Manual)");
            }
        }

        self.update_status_labels();
        self.update_tray_icon_and_tip();
    }

    pub fn manual_clear(&self) {
        log_info!("Manual Clear Proxy requested");
        if let Err(e) = clear_proxy() {
            log_error!("Manual clear proxy failed: {}", e);
            self.lbl_form_error.set_text(&format!("Failed to clear proxy: {}", e));
        } else {
            self.record_switch_time();
            self.show_balloon_notification("Proxy disabled (Manual)");
        }
        self.update_status_labels();
        self.update_tray_icon_and_tip();
    }

    pub fn view_log(&self) {
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

    pub fn quit(&self) {
        log_info!("Application quitting cleanly");
        if let Some(client) = &self.wlan_client {
            client.unregister_notification();
        }
        self.tray.set_visibility(false);
        nwg::stop_thread_dispatch();
    }

    pub fn query_current_network(&self) -> (Option<String>, SsidStatus) {
        if let Some(client) = &self.wlan_client {
            let status = client.get_ssid_status();
            let ssid = match &status {
                SsidStatus::Connected(s) => Some(s.clone()),
                _ => None,
            };
            (ssid, status)
        } else {
            (None, SsidStatus::NoAdapter)
        }
    }

    pub fn on_network_change(&self, can_notify: bool) {
        let (current_ssid, status) = self.query_current_network();
        log_info!(
            "Network state evaluation: SSID={:?}, status={:?}",
            current_ssid,
            status
        );

        let action = {
            let cfg = self.config.borrow();
            decide(&cfg, current_ssid.as_deref())
        };

        let current_proxy = read_proxy().unwrap_or(ProxyState {
            enabled: false,
            server: None,
        });

        if should_execute_action(&action, &current_proxy) {
            match &action {
                Action::Apply(server) => {
                    log_info!("Applying proxy: {}", server);
                    match apply_proxy(server) {
                        Ok(()) => {
                            self.record_switch_time();
                            let ssid_tag = current_ssid.as_deref().unwrap_or("unmatched");
                            if can_notify {
                                self.show_balloon_notification(&format!("Proxy → {} ({})", server, ssid_tag));
                            }
                        }
                        Err(e) => {
                            log_error!("Failed to apply proxy: {}", e);
                            self.lbl_form_error.set_text(&format!("Proxy apply error: {}", e));
                        }
                    }
                }
                Action::Clear => {
                    log_info!("Clearing proxy");
                    match clear_proxy() {
                        Ok(()) => {
                            self.record_switch_time();
                            if can_notify {
                                self.show_balloon_notification("Proxy disabled");
                            }
                        }
                        Err(e) => {
                            log_error!("Failed to clear proxy: {}", e);
                            self.lbl_form_error.set_text(&format!("Proxy clear error: {}", e));
                        }
                    }
                }
                Action::Nothing => {}
            }
        } else {
            log_info!("Proxy state already matches desired action {:?}, skipping registry write", action);
        }

        self.update_status_labels();
        self.update_tray_icon_and_tip();
    }

    fn record_switch_time(&self) {
        unsafe {
            let st = windows::Win32::System::SystemInformation::GetLocalTime();
            *self.last_switch_time.borrow_mut() = format!(
                "{:02}:{:02}:{:02}",
                st.wHour, st.wMinute, st.wSecond
            );
        }
    }

    pub fn update_status_labels(&self) {
        let (_, status) = self.query_current_network();
        match status {
            SsidStatus::Connected(s) => self.lbl_ssid_val.set_text(&s),
            SsidStatus::Disconnected => self.lbl_ssid_val.set_text("Not connected"),
            SsidStatus::NoAdapter => self.lbl_ssid_val.set_text("No Wi-Fi adapter"),
            SsidStatus::PermissionRequired => {
                self.lbl_ssid_val.set_text("SSID unavailable (check Location permission)")
            }
        }

        match read_proxy() {
            Ok(p) if p.enabled => {
                let s = p.server.unwrap_or_else(|| "Unknown".to_string());
                self.lbl_proxy_val.set_text(&s);
            }
            _ => self.lbl_proxy_val.set_text("None"),
        }

        let enabled = self.config.borrow().enabled;
        self.lbl_state_val.set_text(if enabled { "Enabled" } else { "Disabled" });
        self.lbl_time_val.set_text(&self.last_switch_time.borrow());
    }

    pub fn update_tray_icon_and_tip(&self) {
        let enabled = self.config.borrow().enabled;
        if enabled {
            self.tray.set_icon(&self.icon_enabled);
        } else {
            self.tray.set_icon(&self.icon_disabled);
        }

        let (ssid_opt, _) = self.query_current_network();
        let ssid_str = ssid_opt.unwrap_or_else(|| "Not connected".to_string());
        let proxy_str = match read_proxy() {
            Ok(p) if p.enabled => p.server.unwrap_or_else(|| "None".to_string()),
            _ => "None".to_string(),
        };

        let state_str = if enabled { "Auto: ON" } else { "Auto: OFF" };
        let tip = format!("{} | Wi-Fi: {} | Proxy: {}", state_str, ssid_str, proxy_str);
        self.tray.set_tip(&tip);
    }

    pub fn show_balloon_notification(&self, text: &str) {
        if self.config.borrow().notify {
            self.tray.show(text, Some("Wi-Fi Proxy Auto-Switcher"), None, None);
        }
    }
}
