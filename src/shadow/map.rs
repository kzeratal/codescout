use super::layout::{map_path, meta_path};
use crate::error::{CodescoutError, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MapStatus {
    Placeholder,
    Scanned,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MapFrontmatter {
    pub dir: String,
    pub status: MapStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct MapMeta {
    pub synced_at: Option<DateTime<Utc>>,
    pub dir_hash: Option<String>,
    pub files: HashMap<String, String>,
    pub children: HashMap<String, Option<String>>,
}

/// Parse a _MAP.md file into (frontmatter, body).
pub fn read_map(shadow_dir: &Path) -> Result<(MapFrontmatter, String)> {
    let path = map_path(shadow_dir);
    let content = std::fs::read_to_string(&path).map_err(|e| CodescoutError::MalformedMap {
        path: path.clone(),
        reason: e.to_string(),
    })?;

    parse_map_content(&path, &content)
}

pub fn parse_map_content(path: &Path, content: &str) -> Result<(MapFrontmatter, String)> {
    let content = content.trim_start();
    if !content.starts_with("---") {
        return Err(CodescoutError::MalformedMap {
            path: path.to_path_buf(),
            reason: "missing YAML frontmatter".to_string(),
        }
        .into());
    }
    let after = &content[3..];
    let end = after
        .find("\n---")
        .ok_or_else(|| CodescoutError::MalformedMap {
            path: path.to_path_buf(),
            reason: "unclosed frontmatter".to_string(),
        })?;
    let yaml_str = &after[..end];
    let body = after[end + 4..].trim_start_matches('\n').to_string();

    let fm: MapFrontmatter = serde_json::from_str(&serde_yaml_to_json(yaml_str)).map_err(|e| {
        CodescoutError::MalformedMap {
            path: path.to_path_buf(),
            reason: format!("frontmatter parse error: {e}"),
        }
    })?;
    Ok((fm, body))
}

/// Minimal YAML→JSON for the two-field frontmatter we write.
fn serde_yaml_to_json(yaml: &str) -> String {
    let mut map = serde_json::Map::new();
    for line in yaml.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some((k, v)) = line.split_once(':') {
            let key = k.trim().to_string();
            let val = v.trim().to_string();
            map.insert(key, serde_json::Value::String(val));
        }
    }
    serde_json::to_string(&map).unwrap_or_else(|_| "{}".to_string())
}

pub fn write_map(shadow_dir: &Path, fm: &MapFrontmatter, body: &str) -> Result<()> {
    let path = map_path(shadow_dir);
    let status_str = match fm.status {
        MapStatus::Placeholder => "placeholder",
        MapStatus::Scanned => "scanned",
    };
    let content = format!(
        "---\ndir: {}\nstatus: {}\n---\n{}",
        fm.dir, status_str, body
    );
    std::fs::write(&path, content)?;
    Ok(())
}

pub fn write_placeholder_map(shadow_dir: &Path, dir: &str) -> Result<()> {
    let fm = MapFrontmatter {
        dir: dir.to_string(),
        status: MapStatus::Placeholder,
    };
    write_map(shadow_dir, &fm, "")
}

pub fn read_meta(shadow_dir: &Path) -> Result<MapMeta> {
    let path = meta_path(shadow_dir);
    if !path.exists() {
        return Ok(MapMeta::default());
    }
    let content = std::fs::read_to_string(&path)?;
    let meta: MapMeta = serde_json::from_str(&content)?;
    Ok(meta)
}

pub fn write_meta(shadow_dir: &Path, meta: &MapMeta) -> Result<()> {
    let path = meta_path(shadow_dir);
    let content = serde_json::to_string_pretty(meta)?;
    std::fs::write(&path, content)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    use tempfile::tempdir;

    #[test]
    fn parse_map_content_placeholder() {
        let content = "---\ndir: src/foo\nstatus: placeholder\n---\n";
        let (fm, body) = parse_map_content(Path::new("_MAP.md"), content).unwrap();
        assert_eq!(fm.dir, "src/foo");
        assert_eq!(fm.status, MapStatus::Placeholder);
        assert_eq!(body, "");
    }

    #[test]
    fn parse_map_content_scanned_with_body() {
        let content = "---\ndir: src/bar\nstatus: scanned\n---\n## Purpose\n\nDoes stuff\n";
        let (fm, body) = parse_map_content(Path::new("_MAP.md"), content).unwrap();
        assert_eq!(fm.dir, "src/bar");
        assert_eq!(fm.status, MapStatus::Scanned);
        assert!(body.contains("Does stuff"));
    }

    #[test]
    fn parse_map_content_leading_whitespace_tolerated() {
        let content = "\n---\ndir: src/baz\nstatus: placeholder\n---\n";
        let (fm, _) = parse_map_content(Path::new("_MAP.md"), content).unwrap();
        assert_eq!(fm.dir, "src/baz");
    }

    #[test]
    fn parse_map_content_missing_frontmatter_errors() {
        let content = "no frontmatter here\n";
        assert!(parse_map_content(Path::new("_MAP.md"), content).is_err());
    }

    #[test]
    fn parse_map_content_unclosed_frontmatter_errors() {
        let content = "---\ndir: src/foo\nstatus: placeholder\n";
        assert!(parse_map_content(Path::new("_MAP.md"), content).is_err());
    }

    #[test]
    fn write_read_map_roundtrip() {
        let dir = tempdir().unwrap();
        let fm = MapFrontmatter {
            dir: "src/foo".to_string(),
            status: MapStatus::Scanned,
        };
        write_map(dir.path(), &fm, "some body").unwrap();
        let (fm2, body) = read_map(dir.path()).unwrap();
        assert_eq!(fm2.dir, "src/foo");
        assert_eq!(fm2.status, MapStatus::Scanned);
        assert_eq!(body, "some body");
    }

    #[test]
    fn write_placeholder_map_creates_placeholder_status() {
        let dir = tempdir().unwrap();
        write_placeholder_map(dir.path(), "src/commands").unwrap();
        let (fm, body) = read_map(dir.path()).unwrap();
        assert_eq!(fm.dir, "src/commands");
        assert_eq!(fm.status, MapStatus::Placeholder);
        assert_eq!(body, "");
    }

    #[test]
    fn write_read_meta_roundtrip() {
        let dir = tempdir().unwrap();
        let mut files = HashMap::new();
        files.insert("src/foo.rs".to_string(), "abc123".to_string());
        let meta = MapMeta {
            synced_at: None,
            dir_hash: Some("deadbeef".to_string()),
            files,
            children: HashMap::new(),
        };
        write_meta(dir.path(), &meta).unwrap();
        let meta2 = read_meta(dir.path()).unwrap();
        assert_eq!(meta2.dir_hash, Some("deadbeef".to_string()));
        assert_eq!(meta2.files.get("src/foo.rs"), Some(&"abc123".to_string()));
    }

    #[test]
    fn read_meta_missing_returns_default() {
        let dir = tempdir().unwrap();
        let meta = read_meta(dir.path()).unwrap();
        assert!(meta.dir_hash.is_none());
        assert!(meta.files.is_empty());
        assert!(meta.children.is_empty());
    }
}
