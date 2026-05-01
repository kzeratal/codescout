#![allow(dead_code)]
use super::layout::index_path;
use crate::error::Result;
use serde::{Deserialize, Serialize};
use std::fmt;
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EntryKind {
    Function,
    Class,
    Interface,
    Type,
    Const,
    Route,
    Cli,
    Export,
}

impl fmt::Display for EntryKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            Self::Function => "function",
            Self::Class => "class",
            Self::Interface => "interface",
            Self::Type => "type",
            Self::Const => "const",
            Self::Route => "route",
            Self::Cli => "cli",
            Self::Export => "export",
        };
        write!(f, "{s}")
    }
}

impl std::str::FromStr for EntryKind {
    type Err = ();
    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s {
            "function" => Ok(Self::Function),
            "class" => Ok(Self::Class),
            "interface" => Ok(Self::Interface),
            "type" => Ok(Self::Type),
            "const" => Ok(Self::Const),
            "route" => Ok(Self::Route),
            "cli" => Ok(Self::Cli),
            "export" => Ok(Self::Export),
            _ => Err(()),
        }
    }
}

#[derive(Debug, Clone)]
pub struct IndexEntry {
    pub name: String,
    pub kind: EntryKind,
    pub location: String,
}

impl IndexEntry {
    pub fn to_pipe_line(&self) -> String {
        format!("{}|{}|{}", self.name, self.kind, self.location)
    }
}

fn parse_index_line(line: &str) -> Option<IndexEntry> {
    let mut parts = line.splitn(3, '|');
    let name = parts.next()?.trim().to_string();
    let kind_str = parts.next()?.trim();
    let location = parts.next()?.trim().to_string();
    let kind = kind_str.parse().ok()?;
    if name.is_empty() || location.is_empty() {
        return None;
    }
    Some(IndexEntry {
        name,
        kind,
        location,
    })
}

pub fn read_index(shadow_root: &Path) -> Result<Vec<IndexEntry>> {
    let path = index_path(shadow_root);
    if !path.exists() {
        return Ok(Vec::new());
    }
    let content = std::fs::read_to_string(&path)?;
    let entries = content
        .lines()
        .filter(|l| !l.starts_with('#') && !l.is_empty())
        .filter_map(parse_index_line)
        .collect();
    Ok(entries)
}

pub fn write_index(shadow_root: &Path, entries: &[IndexEntry]) -> Result<()> {
    let path = index_path(shadow_root);
    let mut lines = vec!["# name|kind|location".to_string()];
    let mut sorted: Vec<&IndexEntry> = entries.iter().collect();
    sorted.sort_by(|a, b| a.name.cmp(&b.name));
    for e in sorted {
        lines.push(e.to_pipe_line());
    }
    let content = lines.join("\n") + "\n";
    // atomic write via temp file in same dir
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, &content)?;
    std::fs::rename(&tmp, &path)?;
    Ok(())
}

pub fn merge_index(
    old: Vec<IndexEntry>,
    fresh: Vec<IndexEntry>,
    fresh_dirs: &[String],
) -> Vec<IndexEntry> {
    // Keep old entries whose location doesn't fall in any freshly-scanned dir.
    // fresh_dirs are repo-relative dir paths (e.g. "src/commands").
    let mut merged: Vec<IndexEntry> = old
        .into_iter()
        .filter(|e| {
            !fresh_dirs
                .iter()
                .any(|dir| e.location.starts_with(dir.as_str()))
        })
        .collect();
    merged.extend(fresh);
    merged
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn make_entry(name: &str, kind: EntryKind, location: &str) -> IndexEntry {
        IndexEntry {
            name: name.to_string(),
            kind,
            location: location.to_string(),
        }
    }

    #[test]
    fn entry_kind_display_from_str_roundtrip() {
        let kinds = [
            EntryKind::Function,
            EntryKind::Class,
            EntryKind::Interface,
            EntryKind::Type,
            EntryKind::Const,
            EntryKind::Route,
            EntryKind::Cli,
            EntryKind::Export,
        ];
        for kind in &kinds {
            let s = kind.to_string();
            let parsed: EntryKind = s.parse().unwrap();
            assert_eq!(parsed, *kind);
        }
    }

    #[test]
    fn entry_kind_from_str_unknown_errors() {
        assert!("unknown".parse::<EntryKind>().is_err());
        assert!("".parse::<EntryKind>().is_err());
    }

    #[test]
    fn index_entry_to_pipe_line() {
        let entry = make_entry("my_func", EntryKind::Function, "src/lib.rs:10");
        assert_eq!(entry.to_pipe_line(), "my_func|function|src/lib.rs:10");
    }

    #[test]
    fn parse_index_line_valid() {
        let entry = parse_index_line("my_func|function|src/lib.rs:10").unwrap();
        assert_eq!(entry.name, "my_func");
        assert_eq!(entry.kind, EntryKind::Function);
        assert_eq!(entry.location, "src/lib.rs:10");
    }

    #[test]
    fn parse_index_line_with_whitespace_trimmed() {
        let entry = parse_index_line("  my_func  |  class  |  src/lib.rs:5  ").unwrap();
        assert_eq!(entry.name, "my_func");
        assert_eq!(entry.kind, EntryKind::Class);
        assert_eq!(entry.location, "src/lib.rs:5");
    }

    #[test]
    fn parse_index_line_too_few_parts_returns_none() {
        assert!(parse_index_line("my_func|function").is_none());
        assert!(parse_index_line("my_func").is_none());
    }

    #[test]
    fn parse_index_line_bad_kind_returns_none() {
        assert!(parse_index_line("my_func|badkind|src/lib.rs:10").is_none());
    }

    #[test]
    fn parse_index_line_empty_name_returns_none() {
        assert!(parse_index_line("|function|src/lib.rs:10").is_none());
    }

    #[test]
    fn parse_index_line_empty_location_returns_none() {
        assert!(parse_index_line("my_func|function|").is_none());
    }

    #[test]
    fn merge_index_keeps_entries_outside_fresh_dirs() {
        let old = vec![
            make_entry("a", EntryKind::Function, "src/old/file.rs:1"),
            make_entry("b", EntryKind::Class, "src/other/file.rs:2"),
        ];
        let fresh = vec![make_entry("c", EntryKind::Function, "src/new/file.rs:3")];
        let merged = merge_index(old, fresh, &["src/new".to_string()]);
        assert_eq!(merged.len(), 3);
        assert!(merged.iter().any(|e| e.name == "a"));
        assert!(merged.iter().any(|e| e.name == "b"));
        assert!(merged.iter().any(|e| e.name == "c"));
    }

    #[test]
    fn merge_index_drops_stale_entries_in_fresh_dirs() {
        let old = vec![make_entry("old", EntryKind::Function, "src/replaced/file.rs:1")];
        let fresh = vec![make_entry("new", EntryKind::Function, "src/replaced/file.rs:5")];
        let merged = merge_index(old, fresh, &["src/replaced".to_string()]);
        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].name, "new");
    }

    #[test]
    fn merge_index_empty_old_and_empty_fresh() {
        let merged = merge_index(vec![], vec![], &[]);
        assert!(merged.is_empty());
    }

    #[test]
    fn write_read_index_roundtrip() {
        let dir = tempdir().unwrap();
        let entries = vec![
            make_entry("z_func", EntryKind::Function, "a.rs:1"),
            make_entry("a_class", EntryKind::Class, "b.rs:2"),
        ];
        write_index(dir.path(), &entries).unwrap();
        let read = read_index(dir.path()).unwrap();
        assert_eq!(read.len(), 2);
        assert_eq!(read[0].name, "a_class");
        assert_eq!(read[1].name, "z_func");
    }

    #[test]
    fn read_index_skips_comments_and_blank_lines() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("_INDEX.md");
        std::fs::write(
            &path,
            "# name|kind|location\n\nmy_func|function|src/lib.rs:1\n# another comment\n",
        )
        .unwrap();
        let entries = read_index(dir.path()).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "my_func");
    }

    #[test]
    fn read_index_missing_returns_empty() {
        let dir = tempdir().unwrap();
        let entries = read_index(dir.path()).unwrap();
        assert!(entries.is_empty());
    }
}
