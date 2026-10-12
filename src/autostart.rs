use windows::core::{w, PCWSTR};
use windows::Win32::System::Registry::{
    RegCloseKey, RegDeleteValueW, RegOpenKeyExW, RegQueryValueExW, RegSetValueExW, HKEY,
    HKEY_CURRENT_USER, KEY_READ, KEY_WRITE, REG_SZ, REG_VALUE_TYPE,
};

use crate::log_error;

const RUN_SUBKEY: PCWSTR = w!("Software\\Microsoft\\Windows\\CurrentVersion\\Run");
const VALUE_NAME: PCWSTR = w!("WifiProxySwitcher");

pub fn is_autostart_enabled() -> bool {
    unsafe {
        let mut hkey: HKEY = HKEY::default();
        if RegOpenKeyExW(HKEY_CURRENT_USER, RUN_SUBKEY, 0, KEY_READ, &mut hkey).is_err() {
            return false;
        }

        let mut val_type: REG_VALUE_TYPE = REG_VALUE_TYPE::default();
        let status = RegQueryValueExW(
            hkey,
            VALUE_NAME,
            None,
            Some(&mut val_type),
            None,
            None,
        );
        let _ = RegCloseKey(hkey);

        status.is_ok()
    }
}

pub fn set_autostart(enabled: bool) -> Result<(), String> {
    unsafe {
        let mut hkey: HKEY = HKEY::default();
        let status = RegOpenKeyExW(
            HKEY_CURRENT_USER,
            RUN_SUBKEY,
            0,
            KEY_READ | KEY_WRITE,
            &mut hkey,
        );
        if status.is_err() {
            let err = format!("Failed to open Run key in registry: {:?}", status);
            log_error!("{}", err);
            return Err(err);
        }

        let result = if enabled {
            let current_exe = std::env::current_exe().map_err(|e| e.to_string())?;
            let cmd = format!("\"{}\" --hidden", current_exe.to_string_lossy());
            let mut wide_chars: Vec<u16> = cmd.encode_utf16().collect();
            wide_chars.push(0);

            let byte_slice: &[u8] = std::slice::from_raw_parts(
                wide_chars.as_ptr() as *const u8,
                wide_chars.len() * 2,
            );

            let res = RegSetValueExW(
                hkey,
                VALUE_NAME,
                0,
                REG_SZ,
                Some(byte_slice),
            );
            if res.is_err() {
                let err = format!("Failed to set Run key value: {:?}", res);
                log_error!("{}", err);
                Err(err)
            } else {
                Ok(())
            }
        } else {
            let res = RegDeleteValueW(hkey, VALUE_NAME);
            // If it doesn't exist, ignore error (treating as disabled)
            let _ = res;
            Ok(())
        };

        let _ = RegCloseKey(hkey);
        result
    }
}
