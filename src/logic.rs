use crate::config::{Config, UnmatchedBehavior};
use crate::proxy::ProxyState;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    Apply(String),
    Clear,
    Nothing,
}

pub fn decide(config: &Config, current_ssid: Option<&str>) -> Action {
    if !config.enabled {
        return Action::Nothing;
    }

    if let Some(ssid) = current_ssid {
        if let Some(rule) = config.rules.iter().find(|r| r.ssid == ssid) {
            return Action::Apply(rule.proxy.clone());
        }
    }

    match config.unmatched {
        UnmatchedBehavior::Disable => Action::Clear,
        UnmatchedBehavior::Keep => Action::Nothing,
        UnmatchedBehavior::Default => {
            let def = config.default_proxy.trim();
            if def.is_empty() {
                Action::Clear
            } else {
                Action::Apply(config.default_proxy.clone())
            }
        }
    }
}

pub fn should_execute_action(action: &Action, current_state: &ProxyState) -> bool {
    match action {
        Action::Nothing => false,
        Action::Clear => current_state.enabled,
        Action::Apply(target) => {
            !current_state.enabled || current_state.server.as_deref() != Some(target.as_str())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Rule;

    #[test]
    fn test_decision_matrix() {
        let base_rules = vec![
            Rule {
                ssid: "CorpNet".to_string(),
                proxy: "10.0.0.1:8080".to_string(),
            },
        ];

        // 1. When config is disabled: ALWAYS Action::Nothing
        for unmatched in [UnmatchedBehavior::Disable, UnmatchedBehavior::Keep, UnmatchedBehavior::Default] {
            for ssid in [Some("CorpNet"), Some("OtherNet"), None] {
                let config = Config {
                    enabled: false,
                    rules: base_rules.clone(),
                    unmatched,
                    default_proxy: "1.1.1.1:80".to_string(),
                    notify: true,
                    start_with_windows: false,
                };
                assert_eq!(decide(&config, ssid), Action::Nothing);
            }
        }

        // 2. When config is enabled and SSID matches rule: ALWAYS Action::Apply(rule.proxy)
        for unmatched in [UnmatchedBehavior::Disable, UnmatchedBehavior::Keep, UnmatchedBehavior::Default] {
            let config = Config {
                enabled: true,
                rules: base_rules.clone(),
                unmatched,
                default_proxy: "1.1.1.1:80".to_string(),
                notify: true,
                start_with_windows: false,
            };
            assert_eq!(decide(&config, Some("CorpNet")), Action::Apply("10.0.0.1:8080".to_string()));
        }

        // 3. Unmatched or None SSID with "disable"
        let config_disable = Config {
            enabled: true,
            rules: base_rules.clone(),
            unmatched: UnmatchedBehavior::Disable,
            default_proxy: "".to_string(),
            notify: true,
            start_with_windows: false,
        };
        assert_eq!(decide(&config_disable, Some("CoffeeShop")), Action::Clear);
        assert_eq!(decide(&config_disable, None), Action::Clear);

        // 4. Unmatched or None SSID with "keep"
        let config_keep = Config {
            enabled: true,
            rules: base_rules.clone(),
            unmatched: UnmatchedBehavior::Keep,
            default_proxy: "".to_string(),
            notify: true,
            start_with_windows: false,
        };
        assert_eq!(decide(&config_keep, Some("CoffeeShop")), Action::Nothing);
        assert_eq!(decide(&config_keep, None), Action::Nothing);

        // 5. Unmatched or None SSID with "default" (default_proxy non-empty)
        let config_default_set = Config {
            enabled: true,
            rules: base_rules.clone(),
            unmatched: UnmatchedBehavior::Default,
            default_proxy: "192.168.1.1:3128".to_string(),
            notify: true,
            start_with_windows: false,
        };
        assert_eq!(
            decide(&config_default_set, Some("CoffeeShop")),
            Action::Apply("192.168.1.1:3128".to_string())
        );
        assert_eq!(
            decide(&config_default_set, None),
            Action::Apply("192.168.1.1:3128".to_string())
        );

        // 6. Unmatched or None SSID with "default" (default_proxy empty) -> Clear
        let config_default_empty = Config {
            enabled: true,
            rules: base_rules.clone(),
            unmatched: UnmatchedBehavior::Default,
            default_proxy: "".to_string(),
            notify: true,
            start_with_windows: false,
        };
        assert_eq!(decide(&config_default_empty, Some("CoffeeShop")), Action::Clear);
        assert_eq!(decide(&config_default_empty, None), Action::Clear);
    }

    #[test]
    fn test_skip_redundant_writes() {
        let state_enabled_target = ProxyState {
            enabled: true,
            server: Some("10.0.0.1:8080".to_string()),
        };
        let state_enabled_other = ProxyState {
            enabled: true,
            server: Some("10.0.0.2:8080".to_string()),
        };
        let state_disabled = ProxyState {
            enabled: false,
            server: Some("10.0.0.1:8080".to_string()),
        };

        // Apply same proxy when already enabled and matching -> skip (false)
        assert!(!should_execute_action(
            &Action::Apply("10.0.0.1:8080".to_string()),
            &state_enabled_target
        ));

        // Apply different proxy -> execute (true)
        assert!(should_execute_action(
            &Action::Apply("10.0.0.2:8080".to_string()),
            &state_enabled_target
        ));
        assert!(should_execute_action(
            &Action::Apply("10.0.0.1:8080".to_string()),
            &state_enabled_other
        ));

        // Apply when disabled -> execute (true)
        assert!(should_execute_action(
            &Action::Apply("10.0.0.1:8080".to_string()),
            &state_disabled
        ));

        // Clear when disabled -> skip (false)
        assert!(!should_execute_action(&Action::Clear, &state_disabled));

        // Clear when enabled -> execute (true)
        assert!(should_execute_action(&Action::Clear, &state_enabled_target));

        // Nothing -> skip (false)
        assert!(!should_execute_action(&Action::Nothing, &state_enabled_target));
        assert!(!should_execute_action(&Action::Nothing, &state_disabled));
    }
}
