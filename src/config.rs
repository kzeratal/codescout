use crate::error::{CodescoutError, Result};
use crate::shadow::layout::config_path;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ShadowConfig {
    pub version: u32,
    pub target: String,
    pub git_ref: String,
    pub created_at: DateTime<Utc>,
}

impl ShadowConfig {
    pub fn new(target: &Path, git_ref: &str) -> Self {
        Self {
            version: 1,
            target: target.to_string_lossy().to_string(),
            git_ref: git_ref.to_string(),
            created_at: Utc::now(),
        }
    }

    pub fn load(shadow_root: &Path) -> Result<Self> {
        let path = config_path(shadow_root);
        let content = std::fs::read_to_string(&path)?;
        let cfg: Self = serde_json::from_str(&content)?;
        Ok(cfg)
    }

    pub fn save(&self, shadow_root: &Path) -> Result<()> {
        let path = config_path(shadow_root);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let content = serde_json::to_string_pretty(self)?;
        std::fs::write(&path, content)?;
        Ok(())
    }
}

pub fn resolve_shadow(explicit: Option<PathBuf>, real_root: Option<&Path>) -> Result<PathBuf> {
    if let Some(p) = explicit {
        return Ok(p);
    }
    let real = real_root.ok_or_else(|| {
        anyhow::anyhow!("--shadow is required when not running from inside a repo")
    })?;
    let repo_name = real
        .file_name()
        .ok_or_else(|| anyhow::anyhow!("cannot determine repo name from {}", real.display()))?
        .to_string_lossy()
        .to_string();
    let home = dirs::home_dir().ok_or(CodescoutError::NoHomeDir)?;
    Ok(home.join(".codescout").join("projects").join(repo_name))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn resolve_shadow_returns_explicit_path() {
        let p = PathBuf::from("/some/explicit/shadow");
        let result = resolve_shadow(Some(p.clone()), None).unwrap();
        assert_eq!(result, p);
    }

    #[test]
    fn resolve_shadow_no_explicit_no_root_errors() {
        assert!(resolve_shadow(None, None).is_err());
    }

    #[test]
    fn resolve_shadow_derives_from_repo_name() {
        let dir = tempdir().unwrap();
        let result = resolve_shadow(None, Some(dir.path())).unwrap();
        let dir_name = dir.path().file_name().unwrap().to_string_lossy().to_string();
        let result_str = result.to_string_lossy();
        assert!(result_str.contains(".codescout/projects"));
        assert!(result_str.ends_with(&dir_name));
    }

    #[test]
    fn shadow_config_save_load_roundtrip() {
        let dir = tempdir().unwrap();
        let cfg = ShadowConfig::new(Path::new("/some/repo"), "HEAD");
        cfg.save(dir.path()).unwrap();
        let loaded = ShadowConfig::load(dir.path()).unwrap();
        assert_eq!(loaded.target, "/some/repo");
        assert_eq!(loaded.git_ref, "HEAD");
        assert_eq!(loaded.version, 1);
    }

    #[test]
    fn shadow_config_save_creates_codescout_subdir() {
        let dir = tempdir().unwrap();
        let cfg = ShadowConfig::new(Path::new("/repo"), "main");
        cfg.save(dir.path()).unwrap();
        assert!(dir.path().join(".codescout").join("config.json").exists());
    }
}
