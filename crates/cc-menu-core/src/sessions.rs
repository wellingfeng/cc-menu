use crate::config::{AgentKind, AppConfig, Capability};
use crate::workspace::Workspace;
use anyhow::{Context, Result};
use chrono::{DateTime, TimeZone, Utc};
use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct SessionManifest {
    pub session_id: String,
    pub channel: String,
    pub project_path: PathBuf,
    pub title: String,
    pub last_active: DateTime<Utc>,
    pub transcript_path: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct IndexedSession {
    pub session_id: String,
    pub channel: String,
    pub project_path: PathBuf,
    pub title: String,
    pub last_active: DateTime<Utc>,
    pub source_path: PathBuf,
    pub compatibility: Compatibility,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct Compatibility {
    pub native_resume: bool,
    pub context_export: bool,
    pub context_replay: bool,
    pub readable_archive: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ResumeStrategy {
    NativeResume {
        session_id: String,
        target_agent: String,
    },
    AutomaticHandoff {
        archive_dir: PathBuf,
        target_agent: String,
    },
    ContextReplay {
        archive_dir: PathBuf,
        target_agent: String,
    },
    NewAgentReadsArchive {
        archive_dir: PathBuf,
        target_agent: String,
        reason: String,
    },
}

pub fn init_session_db(path: &Path) -> Result<Connection> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    let conn =
        Connection::open(path).with_context(|| format!("failed to open {}", path.display()))?;
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS sessions (
            session_id TEXT PRIMARY KEY,
            channel TEXT NOT NULL,
            project_path TEXT NOT NULL,
            title TEXT NOT NULL,
            last_active INTEGER NOT NULL,
            source_path TEXT NOT NULL,
            native_resume INTEGER NOT NULL,
            context_export INTEGER NOT NULL,
            context_replay INTEGER NOT NULL,
            readable_archive INTEGER NOT NULL
        );",
    )?;
    Ok(conn)
}

pub fn scan_sessions(config: &AppConfig, db_path: &Path) -> Result<Vec<IndexedSession>> {
    let conn = init_session_db(db_path)?;
    let mut indexed = vec![];
    for root in &config.sessions.roots {
        if !root.exists() {
            continue;
        }
        for entry in walk_files(root)? {
            if entry.extension().and_then(|ext| ext.to_str()) != Some("json") {
                continue;
            }
            let data = fs::read_to_string(&entry)
                .with_context(|| format!("failed to read {}", entry.display()))?;
            let Ok(manifest) =
                serde_json::from_str::<SessionManifest>(data.trim_start_matches('\u{feff}'))
            else {
                continue;
            };
            let compatibility = compatibility_for(&manifest.channel);
            let session = IndexedSession {
                session_id: manifest.session_id,
                channel: manifest.channel,
                project_path: manifest.project_path,
                title: manifest.title,
                last_active: manifest.last_active,
                source_path: entry.clone(),
                compatibility,
            };
            upsert_session(&conn, &session)?;
            indexed.push(session);
        }
    }
    indexed.sort_by(|a, b| b.last_active.cmp(&a.last_active));
    Ok(indexed)
}

pub fn sessions_for_project(db_path: &Path, project_path: &Path) -> Result<Vec<IndexedSession>> {
    let conn = init_session_db(db_path)?;
    let mut stmt = conn.prepare(
        "SELECT session_id, channel, project_path, title, last_active, source_path,
                native_resume, context_export, context_replay, readable_archive
         FROM sessions
         WHERE project_path = ?1
         ORDER BY last_active DESC",
    )?;
    let rows = stmt.query_map([project_path.to_string_lossy().to_string()], row_to_session)?;
    let mut sessions = vec![];
    for row in rows {
        sessions.push(row?);
    }
    Ok(sessions)
}

pub fn decide_resume_strategy(
    config: &AppConfig,
    workspace: &Workspace,
    session: &IndexedSession,
    target_agent: &str,
) -> Result<ResumeStrategy> {
    let Some(agent) = config.agent(target_agent) else {
        return Ok(ResumeStrategy::NewAgentReadsArchive {
            archive_dir: standard_archive_dir(workspace, session),
            target_agent: target_agent.to_string(),
            reason: "target agent is not configured".to_string(),
        });
    };

    if same_channel(&session.channel, agent.kind)
        && session.compatibility.native_resume
        && agent.capabilities.contains(&Capability::NativeResume)
    {
        return Ok(ResumeStrategy::NativeResume {
            session_id: session.session_id.clone(),
            target_agent: target_agent.to_string(),
        });
    }

    let archive_dir = export_standard_context(workspace, session)?;
    if session.compatibility.context_export {
        return Ok(ResumeStrategy::AutomaticHandoff {
            archive_dir,
            target_agent: target_agent.to_string(),
        });
    }
    if session.compatibility.context_replay
        && agent.capabilities.contains(&Capability::ContextReplay)
    {
        return Ok(ResumeStrategy::ContextReplay {
            archive_dir,
            target_agent: target_agent.to_string(),
        });
    }
    Ok(ResumeStrategy::NewAgentReadsArchive {
        archive_dir,
        target_agent: target_agent.to_string(),
        reason: "no native resume or replay-compatible context was available".to_string(),
    })
}

pub fn export_standard_context(workspace: &Workspace, session: &IndexedSession) -> Result<PathBuf> {
    let archive_dir = standard_archive_dir(workspace, session);
    fs::create_dir_all(&archive_dir)
        .with_context(|| format!("failed to create {}", archive_dir.display()))?;
    fs::write(
        archive_dir.join("manifest.json"),
        serde_json::to_string_pretty(session)?,
    )?;
    fs::write(
        archive_dir.join("transcript.md"),
        format!(
            "# {}\n\nSource: {}\nProject: {}\n",
            session.title,
            session.source_path.display(),
            session.project_path.display()
        ),
    )?;
    fs::write(
        archive_dir.join("transcript.jsonl"),
        serde_json::json!({
            "session_id": session.session_id,
            "channel": session.channel,
            "title": session.title
        })
        .to_string(),
    )?;
    fs::write(archive_dir.join("files-touched.json"), "[]")?;
    fs::write(
        archive_dir.join("summary.md"),
        format!("Resume summary for {}", session.title),
    )?;
    Ok(archive_dir)
}

fn upsert_session(conn: &Connection, session: &IndexedSession) -> Result<()> {
    conn.execute(
        "INSERT INTO sessions (
            session_id, channel, project_path, title, last_active, source_path,
            native_resume, context_export, context_replay, readable_archive
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
         ON CONFLICT(session_id) DO UPDATE SET
            channel=excluded.channel,
            project_path=excluded.project_path,
            title=excluded.title,
            last_active=excluded.last_active,
            source_path=excluded.source_path,
            native_resume=excluded.native_resume,
            context_export=excluded.context_export,
            context_replay=excluded.context_replay,
            readable_archive=excluded.readable_archive",
        params![
            session.session_id,
            session.channel,
            session.project_path.to_string_lossy(),
            session.title,
            session.last_active.timestamp(),
            session.source_path.to_string_lossy(),
            session.compatibility.native_resume as i64,
            session.compatibility.context_export as i64,
            session.compatibility.context_replay as i64,
            session.compatibility.readable_archive as i64,
        ],
    )?;
    Ok(())
}

fn row_to_session(row: &rusqlite::Row<'_>) -> rusqlite::Result<IndexedSession> {
    let timestamp: i64 = row.get(4)?;
    let last_active = Utc
        .timestamp_opt(timestamp, 0)
        .single()
        .unwrap_or_else(Utc::now);
    Ok(IndexedSession {
        session_id: row.get(0)?,
        channel: row.get(1)?,
        project_path: PathBuf::from(row.get::<_, String>(2)?),
        title: row.get(3)?,
        last_active,
        source_path: PathBuf::from(row.get::<_, String>(5)?),
        compatibility: Compatibility {
            native_resume: row.get::<_, i64>(6)? != 0,
            context_export: row.get::<_, i64>(7)? != 0,
            context_replay: row.get::<_, i64>(8)? != 0,
            readable_archive: row.get::<_, i64>(9)? != 0,
        },
    })
}

fn walk_files(root: &Path) -> Result<Vec<PathBuf>> {
    let mut files = vec![];
    let mut stack = vec![root.to_path_buf()];
    while let Some(path) = stack.pop() {
        for entry in
            fs::read_dir(&path).with_context(|| format!("failed to read {}", path.display()))?
        {
            let entry = entry?;
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else {
                files.push(path);
            }
        }
    }
    Ok(files)
}

fn compatibility_for(channel: &str) -> Compatibility {
    match channel {
        "claude" => Compatibility {
            native_resume: true,
            context_export: true,
            context_replay: true,
            readable_archive: true,
        },
        "codex" => Compatibility {
            native_resume: false,
            context_export: true,
            context_replay: true,
            readable_archive: true,
        },
        "gemini" => Compatibility {
            native_resume: false,
            context_export: false,
            context_replay: true,
            readable_archive: true,
        },
        _ => Compatibility {
            native_resume: false,
            context_export: false,
            context_replay: false,
            readable_archive: true,
        },
    }
}

fn same_channel(channel: &str, kind: AgentKind) -> bool {
    matches!(
        (channel, kind),
        ("claude", AgentKind::Claude) | ("codex", AgentKind::Codex) | ("gemini", AgentKind::Gemini)
    )
}

fn standard_archive_dir(workspace: &Workspace, session: &IndexedSession) -> PathBuf {
    let mut hasher = Sha256::new();
    hasher.update(session.session_id.as_bytes());
    let digest = format!("{:x}", hasher.finalize());
    workspace
        .standard_context_root()
        .join(&session.channel)
        .join(&digest[..16])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{AppConfig, Capability};
    use tempfile::tempdir;

    #[test]
    fn scans_and_filters_sessions_by_project() {
        let dir = tempdir().unwrap();
        let root = dir.path().join("source");
        fs::create_dir_all(&root).unwrap();
        let project = dir.path().join("project");
        fs::create_dir_all(&project).unwrap();
        fs::write(
            root.join("session.json"),
            serde_json::to_string(&SessionManifest {
                session_id: "s1".to_string(),
                channel: "codex".to_string(),
                project_path: project.clone(),
                title: "Fix tests".to_string(),
                last_active: Utc::now(),
                transcript_path: root.join("transcript.jsonl"),
            })
            .unwrap(),
        )
        .unwrap();
        let mut config = AppConfig::default_with_roots(dir.path());
        config.sessions.roots = vec![root];
        let db = dir.path().join("sessions.sqlite3");
        let sessions = scan_sessions(&config, &db).unwrap();
        assert_eq!(sessions.len(), 1);
        let filtered = sessions_for_project(&db, &project).unwrap();
        assert_eq!(filtered[0].title, "Fix tests");
    }

    #[test]
    fn resume_decision_prefers_native_then_handoff() {
        let dir = tempdir().unwrap();
        let workspace = Workspace::new(dir.path().join("app"));
        let mut config = AppConfig::default_with_roots(dir.path());
        config
            .agent_mut("claude")
            .unwrap()
            .capabilities
            .insert(Capability::NativeResume);
        let session = IndexedSession {
            session_id: "s1".to_string(),
            channel: "claude".to_string(),
            project_path: dir.path().to_path_buf(),
            title: "Claude session".to_string(),
            last_active: Utc::now(),
            source_path: dir.path().join("s.json"),
            compatibility: compatibility_for("claude"),
        };
        let native = decide_resume_strategy(&config, &workspace, &session, "claude").unwrap();
        assert!(matches!(native, ResumeStrategy::NativeResume { .. }));
        let handoff = decide_resume_strategy(&config, &workspace, &session, "codex").unwrap();
        assert!(matches!(handoff, ResumeStrategy::AutomaticHandoff { .. }));
    }
}
