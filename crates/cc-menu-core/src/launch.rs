use crate::config::{AppConfig, Capability};
use anyhow::{Context, Result, bail};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct LaunchRequest {
    pub agent_id: String,
    pub cwd: PathBuf,
    pub mode: LaunchMode,
    pub options: LaunchOptions,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum LaunchMode {
    Native,
    Gateway,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub struct LaunchOptions {
    pub account_id: Option<String>,
    pub model: Option<String>,
    pub permission: Option<String>,
    pub effort: Option<String>,
    pub cache: bool,
    pub tts: bool,
    pub extra_env: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct LaunchPlan {
    pub executable: String,
    pub args: Vec<String>,
    pub cwd: PathBuf,
    pub env: BTreeMap<String, String>,
    pub uses_gateway: bool,
    pub route_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct LaunchRecord {
    pub at: DateTime<Utc>,
    pub request: LaunchRequest,
    pub plan: LaunchPlan,
}

pub fn plan_launch(config: &AppConfig, request: LaunchRequest) -> Result<LaunchPlan> {
    let agent = config
        .agent(&request.agent_id)
        .with_context(|| format!("unknown agent {}", request.agent_id))?;
    agent.validate()?;
    if !request.cwd.exists() {
        bail!("cwd does not exist: {}", request.cwd.display());
    }

    let mut args = agent.command.iter().skip(1).cloned().collect::<Vec<_>>();
    let mut env = request.options.extra_env.clone();
    let mut route_id = None;
    let uses_gateway = matches!(request.mode, LaunchMode::Gateway);
    if uses_gateway {
        if !agent.capabilities.contains(&Capability::GatewayLaunch) {
            bail!("agent {} does not support gateway launches", agent.id);
        }
        let account = match request.options.account_id.as_deref() {
            Some(account_id) => agent
                .accounts
                .iter()
                .find(|account| account.id == account_id && account.enabled),
            None => config.active_route(&agent.id).and_then(|route| {
                agent
                    .accounts
                    .iter()
                    .find(|account| account.route.id == route.id && account.enabled)
            }),
        }
        .with_context(|| format!("no enabled account for agent {}", agent.id))?;
        route_id = Some(account.route.id.clone());
        env.insert(
            "OPENAI_BASE_URL".to_string(),
            format!("http://{}:{}/v1", config.gateway.host, config.gateway.port),
        );
        env.insert("CC_MENU_AGENT".to_string(), agent.id.clone());
        env.insert("CC_MENU_ROUTE".to_string(), account.route.id.clone());
        if let Some(model) = request
            .options
            .model
            .or_else(|| agent.default_model.clone())
        {
            env.insert("CC_MENU_MODEL".to_string(), model);
        }
        if request.options.cache {
            args.push("--cache".to_string());
        }
        if request.options.tts {
            args.push("--tts".to_string());
        }
        if let Some(effort) = request.options.effort {
            args.push("--effort".to_string());
            args.push(effort);
        }
        if let Some(permission) = request.options.permission {
            args.push("--permission".to_string());
            args.push(permission);
        }
    }

    Ok(LaunchPlan {
        executable: agent.command[0].clone(),
        args,
        cwd: request.cwd,
        env,
        uses_gateway,
        route_id,
    })
}

pub fn append_launch_record(path: &Path, request: LaunchRequest, plan: LaunchPlan) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    let record = LaunchRecord {
        at: Utc::now(),
        request,
        plan,
    };
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .with_context(|| format!("failed to open {}", path.display()))?;
    writeln!(file, "{}", serde_json::to_string(&record)?)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::AppConfig;
    use tempfile::tempdir;

    #[test]
    fn native_launch_does_not_inject_gateway() {
        let dir = tempdir().unwrap();
        let config = AppConfig::default_with_roots(dir.path());
        let plan = plan_launch(
            &config,
            LaunchRequest {
                agent_id: "codex".to_string(),
                cwd: dir.path().to_path_buf(),
                mode: LaunchMode::Native,
                options: LaunchOptions::default(),
            },
        )
        .unwrap();
        assert!(!plan.uses_gateway);
        assert!(!plan.env.contains_key("OPENAI_BASE_URL"));
    }

    #[test]
    fn gateway_launch_injects_route_and_options() {
        let dir = tempdir().unwrap();
        let config = AppConfig::default_with_roots(dir.path());
        let plan = plan_launch(
            &config,
            LaunchRequest {
                agent_id: "codex".to_string(),
                cwd: dir.path().to_path_buf(),
                mode: LaunchMode::Gateway,
                options: LaunchOptions {
                    cache: true,
                    tts: true,
                    effort: Some("high".to_string()),
                    ..LaunchOptions::default()
                },
            },
        )
        .unwrap();
        assert!(plan.uses_gateway);
        assert_eq!(plan.route_id.as_deref(), Some("codex-openai"));
        assert_eq!(
            plan.env.get("OPENAI_BASE_URL").map(String::as_str),
            Some("http://127.0.0.1:48117/v1")
        );
        assert!(plan.args.contains(&"--cache".to_string()));
        assert!(plan.args.contains(&"--tts".to_string()));
    }
}
