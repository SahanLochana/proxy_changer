use std::collections::BTreeSet;
use std::ffi::c_void;
use windows::Win32::Foundation::HANDLE;
use windows::Win32::NetworkManagement::WiFi::{
    wlan_intf_opcode_current_connection, wlan_interface_state_connected,
    DOT11_SSID, L2_NOTIFICATION_DATA, WLAN_AVAILABLE_NETWORK,
    WLAN_AVAILABLE_NETWORK_LIST, WLAN_CONNECTION_ATTRIBUTES, WLAN_INTERFACE_INFO,
    WLAN_INTERFACE_INFO_LIST, WLAN_NOTIFICATION_SOURCES, WLAN_NOTIFICATION_SOURCE_ACM,
    WLAN_OPCODE_VALUE_TYPE, WLAN_PROFILE_INFO, WLAN_PROFILE_INFO_LIST,
    WlanCloseHandle, WlanEnumInterfaces, WlanFreeMemory, WlanGetAvailableNetworkList,
    WlanGetProfileList, WlanOpenHandle, WlanQueryInterface, WlanRegisterNotification,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SsidStatus {
    Connected(String),
    Disconnected,
    NoAdapter,
    PermissionRequired,
}

pub struct WlanClient {
    handle: HANDLE,
}

impl WlanClient {
    pub fn new() -> Result<Self, String> {
        unsafe {
            let mut negotiated_version = 0u32;
            let mut handle = HANDLE::default();
            let res = WlanOpenHandle(2, None, &mut negotiated_version, &mut handle);
            if res != 0 {
                return Err(format!("WlanOpenHandle failed with error code: {}", res));
            }
            Ok(Self { handle })
        }
    }

    pub fn handle(&self) -> HANDLE {
        self.handle
    }

    pub fn get_ssid_status(&self) -> SsidStatus {
        unsafe {
            let mut p_interface_list: *mut WLAN_INTERFACE_INFO_LIST = std::ptr::null_mut();
            let res = WlanEnumInterfaces(self.handle, None, &mut p_interface_list);
            if res != 0 || p_interface_list.is_null() {
                return SsidStatus::NoAdapter;
            }

            let iface_list = &*p_interface_list;
            if iface_list.dwNumberOfItems == 0 {
                WlanFreeMemory(p_interface_list as *mut c_void);
                return SsidStatus::NoAdapter;
            }

            let mut connected_found = false;
            let mut found_ssid: Option<String> = None;
            let mut empty_ssid_connected = false;

            // Iterate interfaces
            let items: &[WLAN_INTERFACE_INFO] = std::slice::from_raw_parts(
                iface_list.InterfaceInfo.as_ptr(),
                iface_list.dwNumberOfItems as usize,
            );

            for iface in items {
                if iface.isState == wlan_interface_state_connected {
                    connected_found = true;

                    let mut data_size = 0u32;
                    let mut p_data: *mut c_void = std::ptr::null_mut();
                    let mut opcode_type = WLAN_OPCODE_VALUE_TYPE::default();

                    let query_res = WlanQueryInterface(
                        self.handle,
                        &iface.InterfaceGuid,
                        wlan_intf_opcode_current_connection,
                        None,
                        &mut data_size,
                        &mut p_data,
                        Some(&mut opcode_type),
                    );

                    if query_res == 0 && !p_data.is_null() {
                        let conn_attrs = &*(p_data as *const WLAN_CONNECTION_ATTRIBUTES);
                        let ssid_str = parse_dot11_ssid(&conn_attrs.wlanAssociationAttributes.dot11Ssid);
                        WlanFreeMemory(p_data);

                        if let Some(s) = ssid_str {
                            if !s.trim().is_empty() {
                                found_ssid = Some(s);
                                break;
                            } else {
                                empty_ssid_connected = true;
                            }
                        } else {
                            empty_ssid_connected = true;
                        }
                    } else {
                        empty_ssid_connected = true;
                    }
                }
            }

            WlanFreeMemory(p_interface_list as *mut c_void);

            if let Some(ssid) = found_ssid {
                SsidStatus::Connected(ssid)
            } else if connected_found && empty_ssid_connected {
                SsidStatus::PermissionRequired
            } else {
                SsidStatus::Disconnected
            }
        }
    }

    pub fn current_ssid(&self) -> Option<String> {
        match self.get_ssid_status() {
            SsidStatus::Connected(s) => Some(s),
            _ => None,
        }
    }

    pub fn network_list(&self) -> Vec<String> {
        let mut set = BTreeSet::new();

        unsafe {
            let mut p_interface_list: *mut WLAN_INTERFACE_INFO_LIST = std::ptr::null_mut();
            let res = WlanEnumInterfaces(self.handle, None, &mut p_interface_list);
            if res != 0 || p_interface_list.is_null() {
                return Vec::new();
            }

            let iface_list = &*p_interface_list;
            let items: &[WLAN_INTERFACE_INFO] = std::slice::from_raw_parts(
                iface_list.InterfaceInfo.as_ptr(),
                iface_list.dwNumberOfItems as usize,
            );

            for iface in items {
                // 1. Saved profiles
                let mut p_profiles: *mut WLAN_PROFILE_INFO_LIST = std::ptr::null_mut();
                if WlanGetProfileList(self.handle, &iface.InterfaceGuid, None, &mut p_profiles) == 0
                    && !p_profiles.is_null()
                {
                    let prof_list = &*p_profiles;
                    let prof_items: &[WLAN_PROFILE_INFO] = std::slice::from_raw_parts(
                        prof_list.ProfileInfo.as_ptr(),
                        prof_list.dwNumberOfItems as usize,
                    );
                    for prof in prof_items {
                        let name = parse_utf16_null_terminated(&prof.strProfileName);
                        if !name.trim().is_empty() {
                            set.insert(name);
                        }
                    }
                    WlanFreeMemory(p_profiles as *mut c_void);
                }

                // 2. Available visible networks
                let mut p_available: *mut WLAN_AVAILABLE_NETWORK_LIST = std::ptr::null_mut();
                if WlanGetAvailableNetworkList(
                    self.handle,
                    &iface.InterfaceGuid,
                    0,
                    None,
                    &mut p_available,
                ) == 0 && !p_available.is_null()
                {
                    let avail_list = &*p_available;
                    let net_items: &[WLAN_AVAILABLE_NETWORK] = std::slice::from_raw_parts(
                        avail_list.Network.as_ptr(),
                        avail_list.dwNumberOfItems as usize,
                    );
                    for net in net_items {
                        if let Some(ssid) = parse_dot11_ssid(&net.dot11Ssid) {
                            if !ssid.trim().is_empty() {
                                set.insert(ssid);
                            }
                        }
                    }
                    WlanFreeMemory(p_available as *mut c_void);
                }
            }

            WlanFreeMemory(p_interface_list as *mut c_void);
        }

        set.into_iter().collect()
    }

    pub fn register_notification(&self, context: *const c_void) -> Result<(), String> {
        unsafe {
            let mut prev_source = 0u32;
            let res = WlanRegisterNotification(
                self.handle,
                WLAN_NOTIFICATION_SOURCE_ACM,
                false,
                Some(wlan_notification_callback),
                Some(context),
                None,
                Some(&mut prev_source),
            );
            if res != 0 {
                return Err(format!("WlanRegisterNotification failed with code: {}", res));
            }
            Ok(())
        }
    }

    pub fn unregister_notification(&self) {
        unsafe {
            let mut prev_source = 0u32;
            let _ = WlanRegisterNotification(
                self.handle,
                WLAN_NOTIFICATION_SOURCES(0), // WLAN_NOTIFICATION_SOURCE_NONE
                false,
                None,
                None,
                None,
                Some(&mut prev_source),
            );
        }
    }
}

impl Drop for WlanClient {
    fn drop(&mut self) {
        unsafe {
            if !self.handle.is_invalid() {
                self.unregister_notification();
                let _ = WlanCloseHandle(self.handle, None);
            }
        }
    }
}

unsafe extern "system" fn wlan_notification_callback(
    _data: *mut L2_NOTIFICATION_DATA,
    context: *mut c_void,
) {
    if !context.is_null() {
        unsafe {
            let sender = &*(context as *const native_windows_gui::NoticeSender);
            sender.notice();
        }
    }
}

fn parse_dot11_ssid(dot11: &DOT11_SSID) -> Option<String> {
    let len = dot11.uSSIDLength as usize;
    if len == 0 || len > 32 {
        return None;
    }
    let slice = &dot11.ucSSID[..len];
    Some(String::from_utf8_lossy(slice).to_string())
}

fn parse_utf16_null_terminated(slice: &[u16]) -> String {
    let trimmed = match slice.iter().position(|&c| c == 0) {
        Some(pos) => &slice[..pos],
        None => slice,
    };
    String::from_utf16_lossy(trimmed)
}
