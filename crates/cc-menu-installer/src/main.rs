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
}

fn main() -> Result<()> {
    let cli = InstallerCli::parse();
    ensure_payload()?;
    if cli.self_test {
        self_test()?;
        println!("installer self-test passed");
        return Ok(());
    }

    let install_dir = cli.install_dir.unwrap_or(default_install_dir()?);
    if cli.uninstall {
        uninstall(&install_dir)?;
        if !cli.quiet {
            println!("CC Menu uninstalled from {}", install_dir.display());
        }
        return Ok(());
    }

    install(&install_dir)?;
    if !cli.quiet {
        println!("CC Menu installed to {}", install_dir.display());
        println!(
            "Run: {} --workspace <dir> self-test",
            install_dir.join(exe_name()).display()
        );
    }
    Ok(())
}

fn ensure_payload() -> Result<()> {
    if CLI_BYTES.is_empty() {
        bail!("installer was built without an embedded cc-menu payload");
    }
    Ok(())
}

fn install(install_dir: &Path) -> Result<PathBuf> {
    fs::create_dir_all(install_dir)
        .with_context(|| format!("failed to create {}", install_dir.display()))?;
    let exe = install_dir.join(exe_name());
    fs::write(&exe, CLI_BYTES).with_context(|| format!("failed to write {}", exe.display()))?;
    fs::write(
        install_dir.join("install-manifest.json"),
        serde_json::to_string_pretty(&serde_json::json!({
            "app": APP_NAME,
            "version": env!("CARGO_PKG_VERSION"),
            "exe": exe,
            "installed_by": "cc-menu-setup"
        }))?,
    )?;
    Ok(exe)
}

fn uninstall(install_dir: &Path) -> Result<()> {
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
    Ok(())
}

fn self_test() -> Result<()> {
    let temp = tempfile::tempdir()?;
    let install_dir = temp.path().join("cc-menu-install");
    let exe = install(&install_dir)?;
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
    uninstall(&install_dir)?;
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
