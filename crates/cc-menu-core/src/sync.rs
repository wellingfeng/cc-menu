use crate::config::{
    Account, AdapterKind, Agent, AgentKind, AppConfig, Capability, EndpointKind, MenuPlacement,
    ProviderTarget, RouteConfig, RoutingStrategy,
};
use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct CcSwitchExport {
    pub agents: Vec<CcSwitchAgent>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct CcSwitchAgent {
    pub id: String,
    pub display_name: String,
    pub command: Vec<String>,
    pub provider: String,
    pub model: String,
    pub account: String,
    pub endpoint: EndpointKind,
    pub credential_ref: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct SyncPreview {
    pub added: Vec<String>,
    pub updated: Vec<String>,
    pub conflicts: Vec<String>,
    pub skipped: Vec<String>,
}

pub fn preview_cc_switch_sync(config: &AppConfig, path: &Path) -> Result<SyncPreview> {
    let export = read_export(path)?;
    let mut seen = BTreeSet::new();
    let mut preview = SyncPreview {
        added: vec![],
        updated: vec![],
        conflicts: vec![],
        skipped: vec![],
    };
    for imported in export.agents {
        if imported.id.trim().is_empty() || imported.command.is_empty() {
            preview.skipped.push(imported.display_name);
            continue;
        }
        if !seen.insert(imported.id.clone()) {
            preview.conflicts.push(imported.id);
            continue;
        }
        match config.agent(&imported.id) {
            Some(existing) if existing.command != imported.command => {
                preview.updated.push(imported.id);
            }
            Some(_) => preview.skipped.push(imported.id),
            None => preview.added.push(imported.id),
        }
    }
    Ok(preview)
}

pub fn apply_cc_switch_sync(config: &mut AppConfig, path: &Path) -> Result<SyncPreview> {
    let export = read_export(path)?;
    let preview = preview_cc_switch_sync(config, path)?;
    for imported in export.agents {
        if imported.id.trim().is_empty()
            || imported.command.is_empty()
            || preview.conflicts.contains(&imported.id)
        {
            continue;
        }
        let route = RouteConfig {
            id: format!("{}-{}", imported.id, imported.account),
            strategy: RoutingStrategy::Fixed,
            providers: vec![ProviderTarget::new(
                &imported.provider,
                &imported.model,
                imported.endpoint,
                10,
                true,
                50,
            )],
        };
        let account = Account {
            id: route.id.clone(),
            display_name: imported.account.clone(),
            provider: imported.provider.clone(),
            route,
            credential_ref: imported.credential_ref.clone(),
            enabled: true,
        };
        match config.agent_mut(&imported.id) {
            Some(agent) => {
                agent.display_name = imported.display_name.clone();
                agent.command = imported.command.clone();
                agent.provider = Some(imported.provider.clone());
                agent.default_model = Some(imported.model.clone());
                upsert_account(agent, account);
            }
            None => config.agents.push(Agent {
                id: imported.id.clone(),
                display_name: imported.display_name.clone(),
                kind: infer_kind(&imported.id),
                adapter: AdapterKind::Cli,
                command: imported.command.clone(),
                provider: Some(imported.provider.clone()),
                default_model: Some(imported.model.clone()),
                menu: MenuPlacement {
                    top_level: false,
                    group: "cc-switch".to_string(),
                    order: 100,
                },
                capabilities: BTreeSet::from([
                    Capability::GatewayLaunch,
                    Capability::ContextReplay,
                    Capability::DesktopRoute,
                ]),
                accounts: vec![account],
                enabled: true,
            }),
        }
    }
    config.validate()?;
    Ok(preview)
}

fn read_export(path: &Path) -> Result<CcSwitchExport> {
    let data = fs::read_to_string(path)
        .with_context(|| format!("failed to read cc-switch export {}", path.display()))?;
    let export: CcSwitchExport = serde_json::from_str(data.trim_start_matches('\u{feff}'))
        .with_context(|| format!("failed to parse cc-switch export {}", path.display()))?;
    if export.agents.is_empty() {
        bail!("cc-switch export contains no agents");
    }
    Ok(export)
}

fn upsert_account(agent: &mut Agent, account: Account) {
    match agent
        .accounts
        .iter_mut()
        .find(|existing| existing.id == account.id)
    {
        Some(existing) => *existing = account,
        None => agent.accounts.push(account),
    }
}

fn infer_kind(id: &str) -> AgentKind {
    match id {
        "claude" => AgentKind::Claude,
        "codex" => AgentKind::Codex,
        "gemini" => AgentKind::Gemini,
        _ => AgentKind::Custom,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::AppConfig;
    use tempfile::tempdir;

    #[test]
    fn dry_run_reports_added_and_apply_merges_agent() {
        let dir = tempdir().unwrap();
        let export_path = dir.path().join("cc-switch.json");
        fs::write(
            &export_path,
            serde_json::json!({
                "agents": [{
                    "id": "custom-reviewer",
                    "display-name": "Custom Reviewer",
                    "command": ["reviewer"],
                    "provider": "local",
                    "model": "reviewer-v1",
                    "account": "Local",
                    "endpoint": "local",
                    "credential-ref": null
                }]
            })
            .to_string(),
        )
        .unwrap();
        let mut config = AppConfig::default_with_roots(dir.path());
        let preview = preview_cc_switch_sync(&config, &export_path).unwrap();
        assert_eq!(preview.added, vec!["custom-reviewer"]);
        apply_cc_switch_sync(&mut config, &export_path).unwrap();
        assert!(config.agent("custom-reviewer").is_some());
        assert!(!config.agent("custom-reviewer").unwrap().menu.top_level);
    }
}
