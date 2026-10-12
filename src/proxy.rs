use windows::core::{w, PCWSTR};
use windows::Win32::Networking::WinInet::{
    InternetSetOptionW, INTERNET_OPTION_REFRESH, INTERNET_OPTION_SETTINGS_CHANGED,
};
use windows::Win32::System::Registry::{
    RegCloseKey, RegOpenKeyExW, RegQueryValueExW, RegSetValueExW, HKEY, HKEY_CURRENT_USER,
    KEY_READ, KEY_WRITE, REG_DWORD, REG_SZ, REG_VALUE_TYPE,
};

use crate::log_error;

const INTERNET_SETTINGS_SUBKEY: PCWSTR = w!("Software\\Microsoft\\Windows\\CurrentVersion\\Internet Settings");
const PROXY_ENABLE_KEY: PCWSTR = w!("ProxyEnable");
const PROXY_SERVER_KEY: PCWSTR = w!("ProxyServer");

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProxyState {
    pub enabled: bool,
    pub server: Option<String>,
}

pub fn read_proxy() -> Result<ProxyState, String> {
    unsafe {
        let mut hkey: HKEY = HKEY::default();
        let status = RegOpenKeyExW(
            HKEY_CURRENT_USER,
            INTERNET_SETTINGS_SUBKEY,
            0,
            KEY_READ,
            &mut hkey,
        );
        if status.is_err() {
            return Err(format!("Failed to open Internet Settings registry key: {:?}", status));
        }

        let mut enabled_val: u32 = 0;
        let mut val_type: REG_VALUE_TYPE = REG_VALUE_TYPE::default();
        let mut data_len: u32 = std::mem::size_of::<u32>() as u32;

        let status = RegQueryValueExW(
            hkey,
            PROXY_ENABLE_KEY,
            None,
            Some(&mut val_type),
            Some(&mut enabled_val as *mut u32 as *mut u8),
            Some(&mut data_len),
        );
        let enabled = status.is_ok() && enabled_val == 1;

        // Read ProxyServer
        let mut server_buf = vec![0u8; 512];
        let mut server_len: u32 = server_buf.len() as u32;
        let status = RegQueryValueExW(
            hkey,
            PROXY_SERVER_KEY,
            None,
            Some(&mut val_type),
            Some(server_buf.as_mut_ptr()),
            Some(&mut server_len),
        );

        let server = if status.is_ok() && server_len > 0 {
            // REG_SZ is stored as UTF-16 on Windows
            let u16_slice: &[u16] = std::slice::from_raw_parts(
                server_buf.as_ptr() as *const u16,
                (server_len as usize) / 2,
            );
            // Remove null terminators
            let trimmed = match u16_slice.iter().position(|&c| c == 0) {
                Some(pos) => &u16_slice[..pos],
                None => u16_slice,
            };
            let s = String::from_utf16_lossy(trimmed);
            if s.is_empty() {
                None
            } else {
                Some(s)
            }
        } else {
            None
        };

        let _ = RegCloseKey(hkey);

        Ok(ProxyState { enabled, server })
    }
}

pub fn apply_proxy(proxy_server: &str) -> Result<(), String> {
    unsafe {
        let mut hkey: HKEY = HKEY::default();
        let status = RegOpenKeyExW(
            HKEY_CURRENT_USER,
            INTERNET_SETTINGS_SUBKEY,
            0,
            KEY_WRITE,
            &mut hkey,
        );
        if status.is_err() {
            let err = format!("Failed to open registry for writing: {:?}", status);
            log_error!("{}", err);
            return Err(err);
        }

        // Set ProxyEnable = 1
        let enable_val: u32 = 1;
        let status = RegSetValueExW(
            hkey,
            PROXY_ENABLE_KEY,
            0,
            REG_DWORD,
            Some(std::slice::from_raw_parts(
                &enable_val as *const u32 as *const u8,
                std::mem::size_of::<u32>(),
            )),
        );
        if status.is_err() {
            let _ = RegCloseKey(hkey);
            let err = format!("Failed to set ProxyEnable: {:?}", status);
            log_error!("{}", err);
            return Err(err);
        }

        // Set ProxyServer = proxy_server (REG_SZ UTF-16 with null terminator)
        let mut wide_chars: Vec<u16> = proxy_server.encode_utf16().collect();
        wide_chars.push(0);
        let byte_slice: &[u8] = std::slice::from_raw_parts(
            wide_chars.as_ptr() as *const u8,
            wide_chars.len() * 2,
        );

        let status = RegSetValueExW(
            hkey,
            PROXY_SERVER_KEY,
            0,
            REG_SZ,
            Some(byte_slice),
        );
        let _ = RegCloseKey(hkey);

        if status.is_err() {
            let err = format!("Failed to set ProxyServer: {:?}", status);
            log_error!("{}", err);
            return Err(err);
        }

        notify_system_settings_changed()?;
        Ok(())
    }
}

pub fn clear_proxy() -> Result<(), String> {
    unsafe {
        let mut hkey: HKEY = HKEY::default();
        let status = RegOpenKeyExW(
            HKEY_CURRENT_USER,
            INTERNET_SETTINGS_SUBKEY,
            0,
            KEY_WRITE,
            &mut hkey,
        );
        if status.is_err() {
            let err = format!("Failed to open registry for writing: {:?}", status);
            log_error!("{}", err);
            return Err(err);
        }

        // Set ProxyEnable = 0, leave ProxyServer as is
        let enable_val: u32 = 0;
        let status = RegSetValueExW(
            hkey,
            PROXY_ENABLE_KEY,
            0,
            REG_DWORD,
            Some(std::slice::from_raw_parts(
                &enable_val as *const u32 as *const u8,
                std::mem::size_of::<u32>(),
            )),
        );
        let _ = RegCloseKey(hkey);

        if status.is_err() {
            let err = format!("Failed to clear ProxyEnable: {:?}", status);
            log_error!("{}", err);
            return Err(err);
        }

        notify_system_settings_changed()?;
        Ok(())
    }
}

fn notify_system_settings_changed() -> Result<(), String> {
    unsafe {
        // InternetSetOptionW(None, INTERNET_OPTION_SETTINGS_CHANGED /*39*/, None, 0)
        let res1 = InternetSetOptionW(None, INTERNET_OPTION_SETTINGS_CHANGED, None, 0);
        if let Err(e) = res1 {
            let err = format!("InternetSetOptionW(SETTINGS_CHANGED) failed: {:?}", e);
            log_error!("{}", err);
            return Err(err);
        }

        // InternetSetOptionW(None, INTERNET_OPTION_REFRESH /*37*/, None, 0)
        let res2 = InternetSetOptionW(None, INTERNET_OPTION_REFRESH, None, 0);
        if let Err(e) = res2 {
            let err = format!("InternetSetOptionW(REFRESH) failed: {:?}", e);
            log_error!("{}", err);
            return Err(err);
        }

        Ok(())
    }
}
