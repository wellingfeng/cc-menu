use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

pub const APP_NAME: &str = "cc-menu";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct AppConfig {
    pub version: u32,
    pub gateway: GatewayConfig,
    pub agents: Vec<Agent>,
    pub terminal: TerminalConfig,
    pub sessions: SessionScanConfig,
    pub desktop: DesktopConfig,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct GatewayConfig {
    pub host: String,
    pub port: u16,
    pub active_routes: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct TerminalConfig {
    pub windows: String,
    pub macos: String,
    pub linux: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct SessionScanConfig {
    pub roots: Vec<PathBuf>,
    pub interval_seconds: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct DesktopConfig {
    pub realtime_switching: bool,
    pub managed_apps: Vec<DesktopApp>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct DesktopApp {
    pub id: String,
    pub display_name: String,
    pub agent_id: String,
    pub running: bool,
    pub supports_hot_reload: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct Agent {
    pub id: String,
    pub display_name: String,
    pub kind: AgentKind,
    pub adapter: AdapterKind,
    pub command: Vec<String>,
    pub provider: Option<String>,
    pub default_model: Option<String>,
    pub menu: MenuPlacement,
    pub capabilities: BTreeSet<Capability>,
    pub accounts: Vec<Account>,
    pub enabled: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum AgentKind {
    Claude,
    Codex,
    Gemini,
    Custom,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum AdapterKind {
    Cli,
    Desktop,
    Custom,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Capability {
    NativeLaunch,
    GatewayLaunch,
    NativeResume,
    ContextExport,
    ContextReplay,
    Tts,
    Cache,
    DesktopRoute,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct MenuPlacement {
    pub top_level: bool,
    pub group: String,
    pub order: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct Account {
    pub id: String,
    pub display_name: String,
    pub provider: String,
    pub route: RouteConfig,
    pub credential_ref: Option<String>,
    pub enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct RouteConfig {
    pub id: String,
    pub strategy: RoutingStrategy,
    pub providers: Vec<ProviderTarget>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RoutingStrategy {
    Fixed,
    Fallback,
    Race,
    Broadcast,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct ProviderTarget {
    pub provider: String,
    pub model: String,
    pub endpoint: EndpointKind,
    pub priority: u32,
    pub simulate: SimulatedProvider,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum EndpointKind {
    OpenAiCompatible,
    Anthropic,
    Gemini,
    Local,
    Custom,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct SimulatedProvider {
    pub succeeds: bool,
    pub latency_ms: u64,
    pub response: String,
}

impl AppConfig {
    pub fn default_with_roots(data_dir: &Path) -> Self {
        let gateway = GatewayConfig {
            host: "127.0.0.1".to_string(),
            port: 48117,
            active_routes: BTreeMap::from([
                ("claude".to_string(), "claude-official".to_string()),
                ("codex".to_string(), "codex-openai".to_string()),
                ("gemini".to_string(), "gemini-google".to_string()),
            ]),
        };

        let claude_route = RouteConfig {
            id: "claude-official".to_string(),
            strategy: RoutingStrategy::Fixed,
            providers: vec![ProviderTarget::new(
                "anthropic",
                "claude-opus-4",
                EndpointKind::Anthropic,
                10,
                true,
                40,
            )],
        };
        let codex_route = RouteConfig {
            id: "codex-openai".to_string(),
            strategy: RoutingStrategy::Fixed,
            providers: vec![ProviderTarget::new(
                "openai",
                "gpt-5.2-codex",
                EndpointKind::OpenAiCompatible,
                10,
                true,
                35,
            )],
        };
        let gemini_route = RouteConfig {
            id: "gemini-google".to_string(),
            strategy: RoutingStrategy::Fixed,
            providers: vec![ProviderTarget::new(
                "google",
                "gemini-3-pro",
                EndpointKind::Gemini,
                10,
                true,
                45,
            )],
        };

        Self {
            version: 1,
            gateway,
            agents: vec![
                Agent::native(
                    "claude",
                    "Claude Code",
                    AgentKind::Claude,
                    vec!["claude".to_string()],
                    claude_route,
                ),
                Agent::native(
                    "codex",
                    "Codex",
                    AgentKind::Codex,
                    vec!["codex".to_string()],
                    codex_route,
                ),
                Agent::native(
                    "gemini",
                    "Gemini",
                    AgentKind::Gemini,
                    vec!["gemini".to_string()],
                    gemini_route,
                ),
            ],
            terminal: TerminalConfig {
                windows: "wt".to_string(),
                macos: "Terminal.app".to_string(),
                linux: "x-terminal-emulator".to_string(),
            },
            sessions: SessionScanConfig {
                roots: vec![data_dir.join("sample-sessions")],
                interval_seconds: 300,
            },
            desktop: DesktopConfig {
                realtime_switching: true,
                managed_apps: vec![
                    DesktopApp {
                        id: "claude-desktop".to_string(),
                        display_name: "Claude Code Desktop".to_string(),
                        agent_id: "claude".to_string(),
                        running: false,
                        supports_hot_reload: true,
                    },
                    DesktopApp {
                        id: "codex-desktop".to_string(),
                        display_name: "Codex Desktop".to_string(),
                        agent_id: "codex".to_string(),
                        running: false,
                        supports_hot_reload: true,
                    },
                ],
            },
        }
    }

    pub fn load(path: &Path) -> Result<Self> {
        let data = fs::read_to_string(path)
            .with_context(|| format!("failed to read config {}", path.display()))?;
        let config = serde_json::from_str(data.trim_start_matches('\u{feff}'))
            .with_context(|| format!("failed to parse config {}", path.display()))?;
        Ok(config)
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("failed to create {}", parent.display()))?;
        }
        let data = serde_json::to_string_pretty(self)?;
        fs::write(path, data).with_context(|| format!("failed to write {}", path.display()))
    }

    pub fn validate(&self) -> Result<()> {
        if self.version == 0 {
            bail!("config version must be non-zero");
        }
        if self.agents.is_empty() {
            bail!("at least one agent is required");
        }
        let mut ids = BTreeSet::new();
        for agent in &self.agents {
            agent.validate()?;
            if !ids.insert(agent.id.clone()) {
                bail!("duplicate agent id {}", agent.id);
            }
        }
        Ok(())
    }

    pub fn agent(&self, id: &str) -> Option<&Agent> {
        self.agents.iter().find(|agent| agent.id == id)
    }

    pub fn agent_mut(&mut self, id: &str) -> Option<&mut Agent> {
        self.agents.iter_mut().find(|agent| agent.id == id)
    }

    pub fn accounts_for_agent(&self, agent_id: &str) -> Vec<&Account> {
        self.agent(agent_id)
            .map(|agent| {
                agent
                    .accounts
                    .iter()
                    .filter(|account| account.enabled)
                    .collect()
            })
            .unwrap_or_default()
    }

    pub fn route(&self, route_id: &str) -> Option<&RouteConfig> {
        self.agents
            .iter()
            .flat_map(|agent| agent.accounts.iter())
            .find(|account| account.route.id == route_id)
            .map(|account| &account.route)
    }

    pub fn active_route(&self, agent_id: &str) -> Option<&RouteConfig> {
        let route_id = self.gateway.active_routes.get(agent_id)?;
        self.route(route_id)
    }
}

impl Agent {
    pub fn native(
        id: &str,
        display_name: &str,
        kind: AgentKind,
        command: Vec<String>,
        route: RouteConfig,
    ) -> Self {
        let provider = route
            .providers
            .first()
            .map(|target| target.provider.clone());
        let default_model = route.providers.first().map(|target| target.model.clone());
        let account = Account {
            id: route.id.clone(),
            display_name: "Official".to_string(),
            provider: provider.clone().unwrap_or_else(|| "unknown".to_string()),
            route,
            credential_ref: Some(format!("cc-menu/{id}/official")),
            enabled: true,
        };
        Self {
            id: id.to_string(),
            display_name: display_name.to_string(),
            kind,
            adapter: AdapterKind::Cli,
            command,
            provider,
            default_model,
            menu: MenuPlacement {
                top_level: true,
                group: "default".to_string(),
                order: match kind {
                    AgentKind::Claude => 10,
                    AgentKind::Codex => 20,
                    AgentKind::Gemini => 30,
                    AgentKind::Custom => 90,
                },
            },
            capabilities: BTreeSet::from([
                Capability::NativeLaunch,
                Capability::GatewayLaunch,
                Capability::ContextReplay,
                Capability::DesktopRoute,
            ]),
            accounts: vec![account],
            enabled: true,
        }
    }

    pub fn validate(&self) -> Result<()> {
        if self.id.trim().is_empty() {
            bail!("agent id cannot be empty");
        }
        if self.command.is_empty() {
            bail!("agent {} must define a command", self.id);
        }
        if self.command.iter().any(|part| part.trim().is_empty()) {
            bail!("agent {} command contains an empty argument", self.id);
        }
        let mut ids = BTreeSet::new();
        for account in &self.accounts {
            if !ids.insert(account.id.clone()) {
                bail!("agent {} has duplicate account {}", self.id, account.id);
            }
            account.validate()?;
        }
        Ok(())
    }
}

impl Account {
    pub fn validate(&self) -> Result<()> {
        if self.id.trim().is_empty() {
            bail!("account id cannot be empty");
        }
        if self.route.providers.is_empty() {
            bail!("account {} route has no providers", self.id);
        }
        Ok(())
    }
}

impl ProviderTarget {
    pub fn new(
        provider: &str,
        model: &str,
        endpoint: EndpointKind,
        priority: u32,
        succeeds: bool,
        latency_ms: u64,
    ) -> Self {
        Self {
            provider: provider.to_string(),
            model: model.to_string(),
            endpoint,
            priority,
            simulate: SimulatedProvider {
                succeeds,
                latency_ms,
                response: format!("{provider}:{model}"),
            },
        }
    }
}
