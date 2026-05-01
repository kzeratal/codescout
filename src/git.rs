use crate::error::{CodescoutError, Result};
use std::path::{Path, PathBuf};
use tokio::process::Command;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TreeEntryKind {
    Blob,
    Tree,
}

#[derive(Debug, Clone)]
pub struct TreeEntry {
    #[allow(dead_code)]
    pub mode: String,
    pub kind: TreeEntryKind,
    pub oid: String,
    pub path: String,
}

async fn run_git(root: &Path, args: &[&str]) -> Result<Vec<u8>> {
    let out = Command::new("git")
        .args(args)
        .current_dir(root)
        .output()
        .await?;
    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr).to_string();
        return Err(CodescoutError::GitFailed { stderr }.into());
    }
    Ok(out.stdout)
}

pub async fn git_is_repo(path: &Path) -> bool {
    Command::new("git")
        .args(["rev-parse", "--is-inside-work-tree"])
        .current_dir(path)
        .output()
        .await
        .map(|o| o.status.success())
        .unwrap_or(false)
}

pub async fn git_toplevel(path: &Path) -> Result<PathBuf> {
    let out = run_git(path, &["rev-parse", "--show-toplevel"]).await?;
    let s = String::from_utf8(out)?.trim().to_string();
    Ok(PathBuf::from(s))
}

pub async fn git_ls_tree(
    root: &Path,
    git_ref: &str,
    subpath: Option<&str>,
) -> Result<Vec<TreeEntry>> {
    let mut args = vec!["ls-tree", "-r", "-t", "--full-tree", git_ref];
    if let Some(p) = subpath {
        args.push(p);
    }
    let out = run_git(root, &args).await?;
    let text = String::from_utf8(out)?;
    let mut entries = Vec::new();
    for line in text.lines() {
        // format: <mode> SP <type> SP <object> TAB <file>
        let (meta, path) = match line.split_once('\t') {
            Some(p) => p,
            None => continue,
        };
        let parts: Vec<&str> = meta.split_whitespace().collect();
        if parts.len() < 3 {
            continue;
        }
        let kind = match parts[1] {
            "blob" => TreeEntryKind::Blob,
            "tree" => TreeEntryKind::Tree,
            _ => continue,
        };
        entries.push(TreeEntry {
            mode: parts[0].to_string(),
            kind,
            oid: parts[2].to_string(),
            path: path.to_string(),
        });
    }
    Ok(entries)
}

pub async fn git_show(root: &Path, git_ref: &str, path: &str) -> Result<Vec<u8>> {
    let spec = format!("{git_ref}:{path}");
    run_git(root, &["show", &spec]).await
}

#[allow(dead_code)]
pub async fn git_tree_hash(root: &Path, git_ref: &str) -> Result<String> {
    let spec = format!("{git_ref}^{{tree}}");
    let out = run_git(root, &["rev-parse", &spec]).await?;
    Ok(String::from_utf8(out)?.trim().to_string())
}
