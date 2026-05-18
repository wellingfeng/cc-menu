use crate::menu::MenuCache;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PlatformKind {
    Windows,
    MacOs,
    Linux,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct PlatformArtifacts {
    pub platform: PlatformKind,
    pub files: Vec<PathBuf>,
}

pub trait PlatformAdapter {
    fn kind(&self) -> PlatformKind;
    fn generate(&self, menu: &MenuCache, output_dir: &Path) -> Result<PlatformArtifacts>;
}

pub struct WindowsAdapter;
pub struct MacOsAdapter;

impl PlatformAdapter for WindowsAdapter {
    fn kind(&self) -> PlatformKind {
        PlatformKind::Windows
    }

    fn generate(&self, menu: &MenuCache, output_dir: &Path) -> Result<PlatformArtifacts> {
        fs::create_dir_all(output_dir)
            .with_context(|| format!("failed to create {}", output_dir.display()))?;
        let menu_json = output_dir.join("windows-menu-cache.json");
        fs::write(&menu_json, serde_json::to_string_pretty(menu)?)?;
        let reg = output_dir.join("install-user-context-menu.reg");
        fs::write(
            &reg,
            format!(
                "Windows Registry Editor Version 5.00\r\n\r\n[HKEY_CURRENT_USER\\Software\\Classes\\Directory\\Background\\shell\\CCMenu]\r\n\"MUIVerb\"=\"CC-Menu\"\r\n\"SubCommands\"=\"\"\r\n\"GeneratedEntries\"=\"{}\"\r\n",
                menu.top_level.len()
            ),
        )?;
        Ok(PlatformArtifacts {
            platform: self.kind(),
            files: vec![menu_json, reg],
        })
    }
}

impl PlatformAdapter for MacOsAdapter {
    fn kind(&self) -> PlatformKind {
        PlatformKind::MacOs
    }

    fn generate(&self, menu: &MenuCache, output_dir: &Path) -> Result<PlatformArtifacts> {
        fs::create_dir_all(output_dir)
            .with_context(|| format!("failed to create {}", output_dir.display()))?;
        let menu_json = output_dir.join("macos-menu-cache.json");
        fs::write(&menu_json, serde_json::to_string_pretty(menu)?)?;
        let plist = output_dir.join("com.cc-menu.agent.plist");
        fs::write(
            &plist,
            r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
  <key>Label</key><string>com.cc-menu.agent</string>
  <key>ProgramArguments</key><array><string>cc-menu</string><string>service</string></array>
  <key>RunAtLoad</key><true/>
</dict></plist>
"#,
        )?;
        Ok(PlatformArtifacts {
            platform: self.kind(),
            files: vec![menu_json, plist],
        })
    }
}

pub fn generate_platform_artifacts(
    menu: &MenuCache,
    platform: PlatformKind,
    output_dir: &Path,
) -> Result<PlatformArtifacts> {
    match platform {
        PlatformKind::Windows => WindowsAdapter.generate(menu, output_dir),
        PlatformKind::MacOs => MacOsAdapter.generate(menu, output_dir),
        PlatformKind::Linux => {
            fs::create_dir_all(output_dir)
                .with_context(|| format!("failed to create {}", output_dir.display()))?;
            let file = output_dir.join("linux-menu-cache.json");
            fs::write(&file, serde_json::to_string_pretty(menu)?)?;
            Ok(PlatformArtifacts {
                platform,
                files: vec![file],
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::AppConfig;
    use crate::menu::build_menu_cache;
    use tempfile::tempdir;

    #[test]
    fn adapters_generate_auditable_artifacts() {
        let dir = tempdir().unwrap();
        let config = AppConfig::default_with_roots(dir.path());
        let cache = build_menu_cache(&config);
        let artifacts =
            generate_platform_artifacts(&cache, PlatformKind::Windows, dir.path()).unwrap();
        assert_eq!(artifacts.files.len(), 2);
        assert!(artifacts.files.iter().all(|path| path.exists()));
    }
}
