use crate::config::AppConfig;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct DesktopSwitchResult {
    pub app_id: String,
    pub agent_id: String,
    pub route_id: String,
    pub restart_required: bool,
    pub message: String,
}

pub fn switch_desktop_route(
    config: &mut AppConfig,
    agent_id: &str,
    account_id: &str,
) -> Result<Vec<DesktopSwitchResult>> {
    let agent = config
        .agent(agent_id)
        .with_context(|| format!("unknown agent {}", agent_id))?;
    let account = agent
        .accounts
        .iter()
        .find(|account| account.id == account_id && account.enabled)
        .with_context(|| format!("unknown enabled account {} for {}", account_id, agent_id))?;
    let route_id = account.route.id.clone();
    config
        .gateway
        .active_routes
        .insert(agent_id.to_string(), route_id.clone());

    let results = config
        .desktop
        .managed_apps
        .iter()
        .filter(|app| app.agent_id == agent_id)
        .map(|app| {
            let restart_required = app.running && !app.supports_hot_reload;
            DesktopSwitchResult {
                app_id: app.id.clone(),
                agent_id: agent_id.to_string(),
                route_id: route_id.clone(),
                restart_required,
                message: if restart_required {
                    "route updated for new sessions; restart required for the running desktop app"
                        .to_string()
                } else {
                    "route updated and available for new sessions".to_string()
                },
            }
        })
        .collect();
    Ok(results)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::AppConfig;
    use tempfile::tempdir;

    #[test]
    fn running_non_hot_reload_app_requires_restart() {
        let dir = tempdir().unwrap();
        let mut config = AppConfig::default_with_roots(dir.path());
        config.desktop.managed_apps[0].running = true;
        config.desktop.managed_apps[0].supports_hot_reload = false;
        let results = switch_desktop_route(&mut config, "claude", "claude-official").unwrap();
        assert!(results[0].restart_required);
        assert_eq!(
            config
                .gateway
                .active_routes
                .get("claude")
                .map(String::as_str),
            Some("claude-official")
        );
    }
}
