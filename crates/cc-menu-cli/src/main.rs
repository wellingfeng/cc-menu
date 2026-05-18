use anyhow::{Context, Result, bail};
use cc_menu_core::{
    AppConfig, GatewayRequest, LaunchMode, LaunchOptions, LaunchRequest, PlatformKind, Workspace,
    append_launch_record, apply_cc_switch_sync, build_menu_cache, decide_resume_strategy,
    generate_platform_artifacts, plan_launch, preview_cc_switch_sync, read_menu_cache,
    route_request, scan_sessions, sessions_for_project, switch_desktop_route, write_menu_cache,
};
use clap::{Args, Parser, Subcommand, ValueEnum};
use serde_json::json;
use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;

#[derive(Debug, Parser)]
#[command(
    name = "cc-menu",
    version,
    about = "AI coding context menu control plane"
)]
struct Cli {
    #[arg(long, value_name = "DIR", global = true)]
    workspace: Option<PathBuf>,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Subcommand)]
enum Commands {
    Init,
    Config {
        #[command(subcommand)]
        command: ConfigCommand,
    },
    Menu {
        #[command(subcommand)]
        command: MenuCommand,
    },
    Launch(LaunchCommand),
    Gateway {
        #[command(subcommand)]
        command: GatewayCommand,
    },
    Sync {
        #[command(subcommand)]
        command: SyncCommand,
    },
    Sessions {
        #[command(subcommand)]
        command: SessionsCommand,
    },
    Desktop {
        #[command(subcommand)]
        command: DesktopCommand,
    },
    Platform {
        #[command(subcommand)]
        command: PlatformCommand,
    },
    SelfTest,
}

#[derive(Debug, Subcommand)]
enum ConfigCommand {
    Print,
}

#[derive(Debug, Subcommand)]
enum MenuCommand {
    Sync,
    Print {
        #[arg(long, value_enum, default_value_t = OutputFormat::Pretty)]
        format: OutputFormat,
    },
}

#[derive(Debug, Args)]
struct LaunchCommand {
    #[arg(long)]
    agent: String,
    #[arg(long)]
    cwd: PathBuf,
    #[arg(long, value_enum, default_value_t = LaunchModeArg::Native)]
    mode: LaunchModeArg,
    #[arg(long)]
    account: Option<String>,
    #[arg(long)]
    model: Option<String>,
    #[arg(long)]
    effort: Option<String>,
    #[arg(long)]
    permission: Option<String>,
    #[arg(long)]
    cache: bool,
    #[arg(long)]
    tts: bool,
    #[arg(long)]
    dry_run: bool,
}

#[derive(Debug, Subcommand)]
enum GatewayCommand {
    Chat {
        #[arg(long)]
        route: Option<String>,
        #[arg(long, value_enum)]
        strategy: Option<StrategyArg>,
        #[arg(long, default_value = "hello")]
        prompt: String,
    },
    Serve {
        #[arg(long)]
        route: Option<String>,
        #[arg(long, default_value_t = 1)]
        max_requests: usize,
    },
}

#[derive(Debug, Subcommand)]
enum SyncCommand {
    Preview {
        #[arg(long)]
        file: PathBuf,
    },
    Apply {
        #[arg(long)]
        file: PathBuf,
    },
}

#[derive(Debug, Subcommand)]
enum SessionsCommand {
    Scan,
    List {
        #[arg(long)]
        cwd: PathBuf,
    },
    Resume {
        #[arg(long)]
        session: String,
        #[arg(long)]
        target: String,
        #[arg(long)]
        cwd: PathBuf,
    },
}

#[derive(Debug, Subcommand)]
enum DesktopCommand {
    Switch {
        #[arg(long)]
        agent: String,
        #[arg(long)]
        account: String,
    },
}

#[derive(Debug, Subcommand)]
enum PlatformCommand {
    Generate {
        #[arg(long, value_enum)]
        platform: PlatformArg,
        #[arg(long)]
        out: PathBuf,
    },
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum OutputFormat {
    Json,
    Pretty,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum LaunchModeArg {
    Native,
    Gateway,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum StrategyArg {
    Fixed,
    Fallback,
    Race,
    Broadcast,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum PlatformArg {
    Windows,
    Macos,
    Linux,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let workspace = cli
        .workspace
        .map(Workspace::new)
        .map(Ok)
        .unwrap_or_else(Workspace::default)?;

    match cli.command {
        Commands::Init => {
            let config = workspace.init()?;
            sync_menu_and_platform(&workspace, &config)?;
            println!(
                "{}",
                serde_json::to_string_pretty(&json!({
                    "workspace": workspace.root(),
                    "agents": config.agents.len(),
                    "menu_cache": workspace.menu_cache_path()
                }))?
            );
        }
        Commands::Config { command } => match command {
            ConfigCommand::Print => {
                let config = workspace.init()?;
                println!("{}", serde_json::to_string_pretty(&config)?);
            }
        },
        Commands::Menu { command } => match command {
            MenuCommand::Sync => {
                let config = workspace.init()?;
                let cache = build_menu_cache(&config);
                write_menu_cache(&cache, &workspace.menu_cache_path())?;
                println!("{}", serde_json::to_string_pretty(&cache)?);
            }
            MenuCommand::Print { format } => {
                let config = workspace.init()?;
                let cache_path = workspace.menu_cache_path();
                if !cache_path.exists() {
                    write_menu_cache(&build_menu_cache(&config), &cache_path)?;
                }
                let cache = read_menu_cache(&cache_path)?;
                match format {
                    OutputFormat::Json => println!("{}", serde_json::to_string(&cache)?),
                    OutputFormat::Pretty => println!("{}", serde_json::to_string_pretty(&cache)?),
                }
            }
        },
        Commands::Launch(args) => {
            let config = workspace.init()?;
            let mode = match args.mode {
                LaunchModeArg::Native => LaunchMode::Native,
                LaunchModeArg::Gateway => LaunchMode::Gateway,
            };
            let request = LaunchRequest {
                agent_id: args.agent,
                cwd: args.cwd,
                mode,
                options: LaunchOptions {
                    account_id: args.account,
                    model: args.model,
                    permission: args.permission,
                    effort: args.effort,
                    cache: args.cache,
                    tts: args.tts,
                    extra_env: BTreeMap::new(),
                },
            };
            let plan = plan_launch(&config, request.clone())?;
            append_launch_record(&workspace.launch_log_path(), request, plan.clone())?;
            println!("{}", serde_json::to_string_pretty(&plan)?);
            if !args.dry_run {
                println!("launch execution is delegated to platform terminal adapters");
            }
        }
        Commands::Gateway { command } => match command {
            GatewayCommand::Chat {
                route,
                strategy,
                prompt,
            } => {
                let config = workspace.init()?;
                let route = select_route(&config, route, strategy)?;
                let response = route_request(&route, &GatewayRequest { prompt })?;
                println!("{}", serde_json::to_string_pretty(&response)?);
            }
            GatewayCommand::Serve {
                route,
                max_requests,
            } => {
                let config = workspace.init()?;
                let route = select_route(&config, route, None)?;
                serve_gateway(&config, route, max_requests)?;
            }
        },
        Commands::Sync { command } => match command {
            SyncCommand::Preview { file } => {
                let config = workspace.init()?;
                let preview = preview_cc_switch_sync(&config, &file)?;
                println!("{}", serde_json::to_string_pretty(&preview)?);
            }
            SyncCommand::Apply { file } => {
                let mut config = workspace.init()?;
                let preview = apply_cc_switch_sync(&mut config, &file)?;
                workspace.save_config(&config)?;
                write_menu_cache(&build_menu_cache(&config), &workspace.menu_cache_path())?;
                println!("{}", serde_json::to_string_pretty(&preview)?);
            }
        },
        Commands::Sessions { command } => match command {
            SessionsCommand::Scan => {
                let config = workspace.init()?;
                let sessions = scan_sessions(&config, &workspace.session_db_path())?;
                println!("{}", serde_json::to_string_pretty(&sessions)?);
            }
            SessionsCommand::List { cwd } => {
                let sessions = sessions_for_project(&workspace.session_db_path(), &cwd)?;
                println!("{}", serde_json::to_string_pretty(&sessions)?);
            }
            SessionsCommand::Resume {
                session,
                target,
                cwd,
            } => {
                let config = workspace.init()?;
                let sessions = sessions_for_project(&workspace.session_db_path(), &cwd)?;
                let selected = sessions
                    .iter()
                    .find(|item| item.session_id == session)
                    .with_context(|| {
                        format!("session {} not found for {}", session, cwd.display())
                    })?;
                let strategy = decide_resume_strategy(&config, &workspace, selected, &target)?;
                println!("{}", serde_json::to_string_pretty(&strategy)?);
            }
        },
        Commands::Desktop { command } => match command {
            DesktopCommand::Switch { agent, account } => {
                let mut config = workspace.init()?;
                let result = switch_desktop_route(&mut config, &agent, &account)?;
                workspace.save_config(&config)?;
                println!("{}", serde_json::to_string_pretty(&result)?);
            }
        },
        Commands::Platform { command } => match command {
            PlatformCommand::Generate { platform, out } => {
                let config = workspace.init()?;
                let cache = build_menu_cache(&config);
                let platform = match platform {
                    PlatformArg::Windows => PlatformKind::Windows,
                    PlatformArg::Macos => PlatformKind::MacOs,
                    PlatformArg::Linux => PlatformKind::Linux,
                };
                let artifacts = generate_platform_artifacts(&cache, platform, &out)?;
                println!("{}", serde_json::to_string_pretty(&artifacts)?);
            }
        },
        Commands::SelfTest => {
            run_self_test(&workspace)?;
        }
    }
    Ok(())
}

fn sync_menu_and_platform(workspace: &Workspace, config: &AppConfig) -> Result<()> {
    let cache = build_menu_cache(config);
    write_menu_cache(&cache, &workspace.menu_cache_path())?;
    generate_platform_artifacts(
        &cache,
        PlatformKind::Windows,
        &workspace.registry_dir().join("windows"),
    )?;
    generate_platform_artifacts(
        &cache,
        PlatformKind::MacOs,
        &workspace.registry_dir().join("macos"),
    )?;
    Ok(())
}

fn select_route(
    config: &AppConfig,
    route_id: Option<String>,
    strategy: Option<StrategyArg>,
) -> Result<cc_menu_core::RouteConfig> {
    if let Some(route_id) = route_id {
        return config
            .route(&route_id)
            .cloned()
            .with_context(|| format!("unknown route {}", route_id));
    }
    if let Some(strategy) = strategy {
        let mut route = config
            .active_route("codex")
            .cloned()
            .context("codex has no active route")?;
        route.strategy = match strategy {
            StrategyArg::Fixed => cc_menu_core::RoutingStrategy::Fixed,
            StrategyArg::Fallback => cc_menu_core::RoutingStrategy::Fallback,
            StrategyArg::Race => cc_menu_core::RoutingStrategy::Race,
            StrategyArg::Broadcast => cc_menu_core::RoutingStrategy::Broadcast,
        };
        if route.providers.len() == 1 {
            let mut backup = route.providers[0].clone();
            backup.provider = format!("{}-backup", backup.provider);
            backup.priority += 10;
            backup.simulate.latency_ms += 15;
            route.providers.push(backup);
        }
        return Ok(route);
    }
    config
        .active_route("codex")
        .cloned()
        .context("codex has no active route")
}

fn serve_gateway(
    config: &AppConfig,
    route: cc_menu_core::RouteConfig,
    max_requests: usize,
) -> Result<()> {
    let listener = TcpListener::bind((&config.gateway.host[..], config.gateway.port))
        .with_context(|| {
            format!(
                "failed to bind {}:{}",
                config.gateway.host, config.gateway.port
            )
        })?;
    eprintln!(
        "cc-menu gateway listening on http://{}:{}/v1",
        config.gateway.host, config.gateway.port
    );
    for incoming in listener.incoming().take(max_requests) {
        let stream = incoming?;
        handle_gateway_stream(stream, &route)?;
    }
    Ok(())
}

fn handle_gateway_stream(mut stream: TcpStream, route: &cc_menu_core::RouteConfig) -> Result<()> {
    let mut buffer = [0_u8; 8192];
    let size = stream.read(&mut buffer)?;
    let request = String::from_utf8_lossy(&buffer[..size]);
    let path_ok = request.starts_with("POST /v1/chat/completions ")
        || request.starts_with("POST /v1/responses ");
    if !path_ok {
        write_http(&mut stream, 404, json!({"error": "not found"}))?;
        return Ok(());
    }
    let prompt = request
        .split("\r\n\r\n")
        .nth(1)
        .and_then(|body| serde_json::from_str::<serde_json::Value>(body).ok())
        .and_then(extract_prompt)
        .unwrap_or_else(|| "hello".to_string());
    let response = route_request(route, &GatewayRequest { prompt })?;
    let body = json!({
        "id": "cc-menu-local",
        "object": "chat.completion",
        "choices": [{
            "index": 0,
            "message": {"role": "assistant", "content": response.content},
            "finish_reason": "stop"
        }],
        "cc_menu": response
    });
    write_http(&mut stream, 200, body)?;
    Ok(())
}

fn extract_prompt(value: serde_json::Value) -> Option<String> {
    value
        .get("messages")
        .and_then(|messages| messages.as_array())
        .and_then(|messages| messages.last())
        .and_then(|message| message.get("content"))
        .and_then(|content| content.as_str())
        .map(ToString::to_string)
        .or_else(|| {
            value
                .get("input")
                .and_then(|input| input.as_str())
                .map(ToString::to_string)
        })
}

fn write_http(stream: &mut TcpStream, status: u16, body: serde_json::Value) -> Result<()> {
    let body = serde_json::to_string(&body)?;
    let label = match status {
        200 => "OK",
        404 => "Not Found",
        _ => "OK",
    };
    write!(
        stream,
        "HTTP/1.1 {status} {label}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        body.len(),
        body
    )?;
    Ok(())
}

fn run_self_test(workspace: &Workspace) -> Result<()> {
    let config = workspace.init()?;
    sync_menu_and_platform(workspace, &config)?;
    let cache = read_menu_cache(&workspace.menu_cache_path())?;
    let top_labels = cache
        .top_level
        .iter()
        .map(|item| item.label.as_str())
        .collect::<Vec<_>>();
    if top_labels != ["Claude Code", "Codex", "Gemini", "CC-Menu"] {
        bail!("unexpected top-level menu labels: {:?}", top_labels);
    }
    let native = plan_launch(
        &config,
        LaunchRequest {
            agent_id: "codex".to_string(),
            cwd: workspace.root().to_path_buf(),
            mode: LaunchMode::Native,
            options: LaunchOptions::default(),
        },
    )?;
    if native.uses_gateway || native.env.contains_key("OPENAI_BASE_URL") {
        bail!("native launch injected gateway state");
    }
    let gateway = plan_launch(
        &config,
        LaunchRequest {
            agent_id: "codex".to_string(),
            cwd: workspace.root().to_path_buf(),
            mode: LaunchMode::Gateway,
            options: LaunchOptions {
                cache: true,
                tts: true,
                ..LaunchOptions::default()
            },
        },
    )?;
    if !gateway.uses_gateway || gateway.route_id.as_deref() != Some("codex-openai") {
        bail!("gateway launch did not select codex-openai");
    }
    for strategy in [
        StrategyArg::Fixed,
        StrategyArg::Fallback,
        StrategyArg::Race,
        StrategyArg::Broadcast,
    ] {
        let route = select_route(&config, None, Some(strategy))?;
        route_request(
            &route,
            &GatewayRequest {
                prompt: format!("self-test {strategy:?}"),
            },
        )?;
    }
    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "ok": true,
            "workspace": workspace.root(),
            "checks": [
                "config",
                "menu",
                "platform-artifacts",
                "native-launch",
                "gateway-launch",
                "gateway-routing"
            ]
        }))?
    );
    Ok(())
}
