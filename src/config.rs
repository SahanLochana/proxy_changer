use serde::{Deserialize, Serialize};
use std::fs::{self, File};
use std::io::Write;
use std::net::Ipv6Addr;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use crate::log::app_data_dir;
use crate::log_warn;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum UnmatchedBehavior {
    Disable,
    Keep,
    Default,
}

impl Default for UnmatchedBehavior {
    fn default() -> Self {
        UnmatchedBehavior::Disable
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Rule {
    pub ssid: String,
    pub proxy: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Config {
    pub enabled: bool,
    pub rules: Vec<Rule>,
    #[serde(default)]
    pub unmatched: UnmatchedBehavior,
    #[serde(default)]
    pub default_proxy: String,
    #[serde(default = "default_true")]
    pub notify: bool,
    #[serde(default)]
    pub start_with_windows: bool,
}

fn default_true() -> bool {
    true
}

impl Default for Config {
    fn default() -> Self {
        Self {
            enabled: true,
            rules: Vec::new(),
            unmatched: UnmatchedBehavior::Disable,
            default_proxy: String::new(),
            notify: true,
            start_with_windows: false,
        }
    }
}

pub fn config_file_path() -> PathBuf {
    app_data_dir().join("config.json")
}

impl Config {
    pub fn load() -> Self {
        Self::load_from_path(&config_file_path())
    }

    pub fn load_from_path(path: &Path) -> Self {
        if !path.exists() {
            return Self::default();
        }

        match fs::read_to_string(path) {
            Ok(content) => match serde_json::from_str::<Config>(&content) {
                Ok(config) => config,
                Err(err) => {
                    log_warn!("Failed to parse config.json, using defaults: {}", err);
                    Self::default()
                }
            },
            Err(err) => {
                log_warn!("Failed to read config.json, using defaults: {}", err);
                Self::default()
            }
        }
    }

    pub fn save(&self) -> Result<(), String> {
        Self::save_to_path(self, &config_file_path())
    }

    pub fn save_to_path(&self, path: &Path) -> Result<(), String> {
        let parent = path
            .parent()
            .ok_or_else(|| "Invalid config path parent".to_string())?;
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;

        let json = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        let tmp_path = path.with_extension("tmp");

        {
            let mut file = File::create(&tmp_path).map_err(|e| e.to_string())?;
            file.write_all(json.as_bytes()).map_err(|e| e.to_string())?;
            file.flush().map_err(|e| e.to_string())?;
        }

        if path.exists() {
            let _ = fs::remove_file(path);
        }

        fs::rename(&tmp_path, path).map_err(|e| {
            let _ = fs::remove_file(&tmp_path);
            e.to_string()
        })?;

        Ok(())
    }
}

pub fn validate_proxy(proxy: &str) -> Result<(), String> {
    if proxy.is_empty() {
        return Err("Proxy cannot be empty".to_string());
    }
    if proxy.contains(char::is_whitespace) {
        return Err("Proxy cannot contain whitespace".to_string());
    }
    if proxy.contains("://") {
        return Err("Proxy cannot contain a URL scheme (e.g. http://)".to_string());
    }

    let (_host, port_str) = if proxy.starts_with('[') {
        let closing = proxy
            .find(']')
            .ok_or_else(|| "Missing closing bracket in IPv6 address".to_string())?;
        let ipv6_part = &proxy[1..closing];
        if !proxy[closing + 1..].starts_with(':') {
            return Err("Missing port separator ':' after [IPv6]".to_string());
        }
        let port_part = &proxy[closing + 2..];

        // Validate IPv6
        if Ipv6Addr::from_str(ipv6_part).is_err() {
            return Err("Invalid IPv6 address".to_string());
        }

        (ipv6_part, port_part)
    } else {
        if proxy.matches(':').count() != 1 {
            return Err("Proxy must be in format host:port".to_string());
        }
        let mut parts = proxy.split(':');
        let host_part = parts.next().unwrap();
        let port_part = parts.next().unwrap();

        if host_part.is_empty() {
            return Err("Host cannot be empty".to_string());
        }

        // Host must contain only valid characters: [A-Za-z0-9.-]
        for c in host_part.chars() {
            if !(c.is_ascii_alphanumeric() || c == '.' || c == '-') {
                return Err(format!("Invalid character in host: '{}'", c));
            }
        }

        (host_part, port_part)
    };

    if port_str.is_empty() {
        return Err("Port cannot be empty".to_string());
    }
    if !port_str.chars().all(|c| c.is_ascii_digit()) {
        return Err("Port must contain only digits".to_string());
    }

    match port_str.parse::<u32>() {
        Ok(port) if (1..=65535).contains(&port) => Ok(()),
        _ => Err("Port must be an integer between 1 and 65535".to_string()),
    }
}

pub fn validate_ssid(ssid: &str, existing_rules: &[Rule], editing_index: Option<usize>) -> Result<String, String> {
    let trimmed = ssid.trim();
    if trimmed.is_empty() {
        return Err("SSID cannot be empty".to_string());
    }
    if trimmed.as_bytes().len() > 32 {
        return Err("SSID cannot exceed 32 bytes".to_string());
    }

    for (i, rule) in existing_rules.iter().enumerate() {
        if Some(i) != editing_index && rule.ssid == trimmed {
            return Err(format!("Rule for SSID \"{}\" already exists. Edit the existing rule instead.", trimmed));
        }
    }

    Ok(trimmed.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_proxy_cases() {
        // Valid cases from SPEC §10
        assert!(validate_proxy("1.2.3.4:80").is_ok());
        assert!(validate_proxy("proxy.corp:8080").is_ok());
        assert!(validate_proxy("[::1]:3128").is_ok());

        // Invalid cases from SPEC §10
        assert!(validate_proxy("host").is_err());
        assert!(validate_proxy("host:0").is_err());
        assert!(validate_proxy("host:70000").is_err());
        assert!(validate_proxy("http://h:80").is_err());
        assert!(validate_proxy("a b:80").is_err());
        assert!(validate_proxy("").is_err());
    }

    #[test]
    fn test_validate_ssid() {
        let rules = vec![Rule {
            ssid: "Office_5G".to_string(),
            proxy: "10.0.0.1:8080".to_string(),
        }];

        // Valid
        assert_eq!(validate_ssid("Home_WiFi", &rules, None).unwrap(), "Home_WiFi");
        assert_eq!(validate_ssid("  Home_WiFi  ", &rules, None).unwrap(), "Home_WiFi");

        // Duplicate rejected
        assert!(validate_ssid("Office_5G", &rules, None).is_err());

        // Duplicate allowed when editing same rule
        assert!(validate_ssid("Office_5G", &rules, Some(0)).is_ok());

        // Empty rejected
        assert!(validate_ssid("", &rules, None).is_err());
        assert!(validate_ssid("   ", &rules, None).is_err());

        // Max 32 bytes
        let long_ssid = "a".repeat(33);
        assert!(validate_ssid(&long_ssid, &rules, None).is_err());
        let exact_32 = "a".repeat(32);
        assert!(validate_ssid(&exact_32, &rules, None).is_ok());
    }

    #[test]
    fn test_config_roundtrip_and_defaults() {
        let temp_dir = std::env::temp_dir().join("wifi_proxy_test_1");
        let _ = fs::create_dir_all(&temp_dir);
        let config_path = temp_dir.join("test_config.json");

        let mut config = Config::default();
        config.enabled = true;
        config.unmatched = UnmatchedBehavior::Default;
        config.default_proxy = "192.168.1.1:8080".to_string();
        config.rules.push(Rule {
            ssid: "MyNet".to_string(),
            proxy: "10.0.0.1:3128".to_string(),
        });

        assert!(config.save_to_path(&config_path).is_ok());

        let loaded = Config::load_from_path(&config_path);
        assert_eq!(config, loaded);

        // Corrupt file fallback
        fs::write(&config_path, "invalid json content {[").unwrap();
        let fallback = Config::load_from_path(&config_path);
        assert_eq!(fallback, Config::default());

        let _ = fs::remove_dir_all(&temp_dir);
    }
}
