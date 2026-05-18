use crate::config::{Agent, AgentKind, AppConfig};
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct MenuCache {
    pub version: u32,
    pub top_level: Vec<MenuItem>,
    pub cc_menu: Vec<MenuItem>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct MenuItem {
    pub id: String,
    pub label: String,
    pub action: MenuAction,
    pub children: Vec<MenuItem>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum MenuAction {
    NativeLaunch {
        agent_id: String,
    },
    OpenStartOptions,
    SyncMenus,
    SyncAgents,
    OpenSettings {
        tab: String,
    },
    OpenSessions,
    SwitchRoute {
        agent_id: String,
        account_id: String,
    },
    Separator,
}

pub fn build_menu_cache(config: &AppConfig) -> MenuCache {
    let mut top_agents: Vec<&Agent> = config
        .agents
        .iter()
        .filter(|agent| agent.enabled && agent.menu.top_level && is_default_top_level(agent))
        .collect();
    top_agents.sort_by_key(|agent| agent.menu.order);

    let mut top_level: Vec<MenuItem> = top_agents
        .into_iter()
        .map(|agent| MenuItem {
            id: format!("launch-{}", agent.id),
            label: agent.display_name.clone(),
            action: MenuAction::NativeLaunch {
                agent_id: agent.id.clone(),
            },
            children: vec![],
        })
        .collect();
    top_level.push(MenuItem {
        id: "cc-menu".to_string(),
        label: "CC-Menu".to_string(),
        action: MenuAction::OpenSettings {
            tab: "dashboard".to_string(),
        },
        children: build_cc_menu(config),
    });

    MenuCache {
        version: config.version,
        top_level,
        cc_menu: build_cc_menu(config),
    }
}

pub fn write_menu_cache(cache: &MenuCache, path: &Path) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    fs::write(path, serde_json::to_string_pretty(cache)?)
        .with_context(|| format!("failed to write {}", path.display()))
}

pub fn read_menu_cache(path: &Path) -> Result<MenuCache> {
    let data =
        fs::read_to_string(path).with_context(|| format!("failed to read {}", path.display()))?;
    Ok(serde_json::from_str(&data)?)
}

fn build_cc_menu(config: &AppConfig) -> Vec<MenuItem> {
    let mut items = vec![
        MenuItem {
            id: "start-options".to_string(),
            label: "Start with Options".to_string(),
            action: MenuAction::OpenStartOptions,
            children: vec![],
        },
        MenuItem {
            id: "sessions".to_string(),
            label: "Sessions".to_string(),
            action: MenuAction::OpenSessions,
            children: vec![],
        },
        MenuItem {
            id: "sync-agents".to_string(),
            label: "Sync Agents from CC-Switch".to_string(),
            action: MenuAction::SyncAgents,
            children: vec![],
        },
        MenuItem {
            id: "sync-menus".to_string(),
            label: "Sync Menus".to_string(),
            action: MenuAction::SyncMenus,
            children: vec![],
        },
        MenuItem {
            id: "settings".to_string(),
            label: "Settings".to_string(),
            action: MenuAction::OpenSettings {
                tab: "agents".to_string(),
            },
            children: vec![],
        },
    ];

    for agent in config.agents.iter().filter(|agent| agent.enabled) {
        let account_items: Vec<MenuItem> = agent
            .accounts
            .iter()
            .filter(|account| account.enabled)
            .map(|account| MenuItem {
                id: format!("switch-{}-{}", agent.id, account.id),
                label: format!("{}: {}", agent.display_name, account.display_name),
                action: MenuAction::SwitchRoute {
                    agent_id: agent.id.clone(),
                    account_id: account.id.clone(),
                },
                children: vec![],
            })
            .collect();
        if !account_items.is_empty() {
            items.push(MenuItem {
                id: format!("accounts-{}", agent.id),
                label: format!("{} accounts", agent.display_name),
                action: MenuAction::Separator,
                children: account_items,
            });
        }
    }

    items
}

fn is_default_top_level(agent: &Agent) -> bool {
    matches!(
        agent.kind,
        AgentKind::Claude | AgentKind::Codex | AgentKind::Gemini
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::AppConfig;
    use tempfile::tempdir;

    #[test]
    fn default_menu_has_only_four_top_level_entries() {
        let dir = tempdir().unwrap();
        let config = AppConfig::default_with_roots(dir.path());
        let cache = build_menu_cache(&config);
        let labels: Vec<_> = cache
            .top_level
            .iter()
            .map(|item| item.label.as_str())
            .collect();
        assert_eq!(labels, vec!["Claude Code", "Codex", "Gemini", "CC-Menu"]);
    }

    #[test]
    fn cc_menu_contains_options_sync_settings_and_accounts() {
        let dir = tempdir().unwrap();
        let config = AppConfig::default_with_roots(dir.path());
        let cache = build_menu_cache(&config);
        let ids: Vec<_> = cache.cc_menu.iter().map(|item| item.id.as_str()).collect();
        assert!(ids.contains(&"start-options"));
        assert!(ids.contains(&"sync-menus"));
        assert!(ids.contains(&"sync-agents"));
        assert!(ids.contains(&"settings"));
        assert!(ids.contains(&"accounts-codex"));
    }
}
