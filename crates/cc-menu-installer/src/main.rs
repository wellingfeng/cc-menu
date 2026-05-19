use anyhow::{Context, Result, bail};
use cc_menu_core::config::APP_NAME;
use clap::Parser;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

#[cfg(feature = "embedded-payload")]
const CLI_BYTES: &[u8] = include_bytes!(env!("CC_MENU_CLI_EXE"));
#[cfg(not(feature = "embedded-payload"))]
const CLI_BYTES: &[u8] = b"";

#[derive(Debug, Parser)]
#[command(name = "cc-menu-setup", version, about = "CC Menu per-user installer")]
struct InstallerCli {
    #[arg(long)]
    install_dir: Option<PathBuf>,
    #[arg(long)]
    uninstall: bool,
    #[arg(long)]
    self_test: bool,
    #[arg(long)]
    quiet: bool,
    #[arg(long, hide = true)]
    registry_prefix: Option<String>,
    #[arg(long, hide = true)]
    wait: bool,
}

fn main() -> Result<()> {
    let launched_without_args = std::env::args_os().len() == 1;
    let cli = InstallerCli::parse();
    ensure_payload()?;
    if cli.self_test {
        self_test()?;
        println!("installer self-test passed");
        return Ok(());
    }

    let install_dir = cli.install_dir.unwrap_or(default_install_dir()?);
    let registry_prefix = cli.registry_prefix.unwrap_or_else(|| "CCMenu".to_string());
    if cli.uninstall {
        uninstall(&install_dir, &registry_prefix)?;
        if !cli.quiet {
            println!("CC Menu uninstalled from {}", install_dir.display());
        }
        return Ok(());
    }

    install(&install_dir, &registry_prefix)?;
    if !cli.quiet {
        println!("CC Menu installed to {}", install_dir.display());
        println!(
            "Run: {} --workspace <dir> self-test",
            install_dir.join(exe_name()).display()
        );
        println!();
        println!(
            "Tip: run the installer with --self-test to verify installation in a temporary directory."
        );
        if launched_without_args || cli.wait {
            pause_for_double_click();
        }
    }
    Ok(())
}

fn ensure_payload() -> Result<()> {
    if CLI_BYTES.is_empty() {
        bail!("installer was built without an embedded cc-menu payload");
    }
    Ok(())
}

fn install(install_dir: &Path, registry_prefix: &str) -> Result<PathBuf> {
    fs::create_dir_all(install_dir)
        .with_context(|| format!("failed to create {}", install_dir.display()))?;
    let exe = install_dir.join(exe_name());
    fs::write(&exe, CLI_BYTES).with_context(|| format!("failed to write {}", exe.display()))?;
    install_context_menu(&exe, registry_prefix)?;
    fs::write(
        install_dir.join("install-manifest.json"),
        serde_json::to_string_pretty(&serde_json::json!({
            "app": APP_NAME,
            "version": env!("CARGO_PKG_VERSION"),
            "exe": exe,
            "installed_by": "cc-menu-setup"
        }))?,
    )?;
    notify_shell_refresh();
    Ok(exe)
}

fn uninstall(install_dir: &Path, registry_prefix: &str) -> Result<()> {
    uninstall_context_menu(registry_prefix)?;
    if install_dir.exists() {
        let resolved = install_dir
            .canonicalize()
            .with_context(|| format!("failed to resolve {}", install_dir.display()))?;
        let safe_hint = APP_NAME.to_ascii_lowercase();
        let resolved_text = resolved.to_string_lossy().to_ascii_lowercase();
        if !resolved_text.contains(&safe_hint) {
            bail!(
                "refusing to uninstall a directory that does not look like CC Menu: {}",
                resolved.display()
            );
        }
        fs::remove_dir_all(&resolved)
            .with_context(|| format!("failed to remove {}", resolved.display()))?;
    }
    notify_shell_refresh();
    Ok(())
}

fn self_test() -> Result<()> {
    let temp = tempfile::tempdir()?;
    let install_dir = temp.path().join("cc-menu-install");
    let registry_prefix = "CCMenuSelfTest";
    let exe = install(&install_dir, registry_prefix)?;
    verify_context_menu_registry(registry_prefix)?;
    let workspace = temp.path().join("workspace");
    let output = Command::new(&exe)
        .arg("--workspace")
        .arg(&workspace)
        .arg("self-test")
        .output()
        .with_context(|| format!("failed to run {}", exe.display()))?;
    if !output.status.success() {
        bail!(
            "installed cc-menu self-test failed\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
    uninstall(&install_dir, registry_prefix)?;
    if install_dir.exists() {
        bail!(
            "installer self-test did not remove {}",
            install_dir.display()
        );
    }
    Ok(())
}

fn default_install_dir() -> Result<PathBuf> {
    let base = dirs::data_local_dir()
        .or_else(dirs::home_dir)
        .context("could not resolve install directory")?;
    Ok(base.join("Programs").join(APP_NAME))
}

fn exe_name() -> &'static str {
    if cfg!(windows) {
        "cc-menu.exe"
    } else {
        "cc-menu"
    }
}

fn pause_for_double_click() {
    use std::io::{self, Write};

    println!("Press Enter to close this installer...");
    let _ = io::stdout().flush();
    let mut input = String::new();
    let _ = io::stdin().read_line(&mut input);
}

fn install_context_menu(exe: &Path, registry_prefix: &str) -> Result<()> {
    if !cfg!(windows) {
        return Ok(());
    }

    let exe = exe
        .canonicalize()
        .unwrap_or_else(|_| exe.to_path_buf())
        .to_string_lossy()
        .trim_start_matches(r"\\?\")
        .to_string();
    for root in [
        (r"HKCU\Software\Classes\Directory\Background\shell", "%V"),
        (r"HKCU\Software\Classes\Directory\shell", "%1"),
        (r"HKCU\Software\Classes\Folder\shell", "%1"),
        (r"HKCU\Software\Classes\Drive\shell", "%1"),
    ] {
        install_menu_item(
            root.0,
            &format!("{registry_prefix}Claude"),
            "Claude Code",
            &exe,
            &format!(
                r#"cmd.exe /d /s /k ""{}" launch --agent claude --cwd "{}" --mode native""#,
                exe, root.1
            ),
        )?;
        install_menu_item(
            root.0,
            &format!("{registry_prefix}Codex"),
            "Codex",
            &exe,
            &format!(
                r#"cmd.exe /d /s /k ""{}" launch --agent codex --cwd "{}" --mode native""#,
                exe, root.1
            ),
        )?;
        install_menu_item(
            root.0,
            &format!("{registry_prefix}Gemini"),
            "Gemini",
            &exe,
            &format!(
                r#"cmd.exe /d /s /k ""{}" launch --agent gemini --cwd "{}" --mode native""#,
                exe, root.1
            ),
        )?;
        install_menu_item(
            root.0,
            registry_prefix,
            "CC-Menu",
            &exe,
            &format!(r#"cmd.exe /d /s /k ""{}" menu print""#, exe),
        )?;
    }
    Ok(())
}

fn install_menu_item(
    root: &str,
    key_name: &str,
    label: &str,
    icon: &str,
    command: &str,
) -> Result<()> {
    let key = format!(r"{root}\{key_name}");
    reg_add(&key, Some("MUIVerb"), label)?;
    reg_add(&key, Some("Icon"), icon)?;
    reg_add(&key, Some("Position"), "Top")?;
    reg_add(&format!(r"{key}\command"), None, command)?;
    Ok(())
}

fn uninstall_context_menu(registry_prefix: &str) -> Result<()> {
    if !cfg!(windows) {
        return Ok(());
    }
    for root in [
        r"HKCU\Software\Classes\Directory\Background\shell",
        r"HKCU\Software\Classes\Directory\shell",
        r"HKCU\Software\Classes\Folder\shell",
        r"HKCU\Software\Classes\Drive\shell",
    ] {
        for key_name in [
            format!("{registry_prefix}Claude"),
            format!("{registry_prefix}Codex"),
            format!("{registry_prefix}Gemini"),
            registry_prefix.to_string(),
        ] {
            reg_delete(&format!(r"{root}\{key_name}"))?;
        }
    }
    Ok(())
}

fn verify_context_menu_registry(registry_prefix: &str) -> Result<()> {
    if !cfg!(windows) {
        return Ok(());
    }
    for key in build_verification_keys(registry_prefix) {
        let status = Command::new("reg")
            .args(["query", &key])
            .status()
            .with_context(|| format!("failed to query registry key {key}"))?;
        if !status.success() {
            bail!("context menu registry key was not created: {key}");
        }
    }
    Ok(())
}

fn build_verification_keys(registry_prefix: &str) -> Vec<String> {
    [
        r"HKCU\Software\Classes\Directory\Background\shell",
        r"HKCU\Software\Classes\Directory\shell",
        r"HKCU\Software\Classes\Folder\shell",
    ]
    .into_iter()
    .flat_map(|root| {
        [
            format!(r"{root}\{registry_prefix}"),
            format!(r"{root}\{registry_prefix}Codex"),
        ]
    })
    .collect()
}

fn reg_add(key: &str, name: Option<&str>, value: &str) -> Result<()> {
    let mut command = Command::new("reg");
    command.args(["add", key, "/f"]);
    match name {
        Some(name) => {
            command.args(["/v", name]);
        }
        None => {
            command.arg("/ve");
        }
    }
    let status = command
        .args(["/d", value])
        .status()
        .with_context(|| format!("failed to write registry key {key}"))?;
    if !status.success() {
        bail!("reg add failed for {key}");
    }
    Ok(())
}

fn reg_delete(key: &str) -> Result<()> {
    let status = Command::new("reg")
        .args(["delete", key, "/f"])
        .status()
        .with_context(|| format!("failed to delete registry key {key}"))?;
    if !status.success() {
        let query_status = Command::new("reg").args(["query", key]).status();
        if matches!(query_status, Ok(status) if status.success()) {
            bail!("reg delete failed for {key}");
        }
    }
    Ok(())
}

fn notify_shell_refresh() {
    if !cfg!(windows) {
        return;
    }
    let _ = Command::new("ie4uinit.exe").arg("-show").status();
}
