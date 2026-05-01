use crate::claude::{format_summary_body, summarize, DirSummary};
use crate::config::ShadowConfig;
use crate::error::Result;
use crate::git::{git_ls_tree, git_show, TreeEntryKind};
use crate::prompt::{ChildSummary, FileContent};
use crate::shadow::index::IndexEntry;
use crate::shadow::layout::map_path;
use crate::shadow::map::{read_meta, write_map, write_meta, MapFrontmatter, MapMeta, MapStatus};
use crate::symbols::SymbolExtractor;
use anyhow::Context;
use futures::future::join_all;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tokio::sync::Semaphore;

const MAX_FILE_BYTES: usize = 12 * 1024;

pub struct ScanOptions {
    pub shadow_root: PathBuf,
    pub real_root: PathBuf,
    pub config: ShadowConfig,
    pub scope: Option<PathBuf>,
    pub concurrency: usize,
    pub model: String,
    pub force: bool,
}

struct ScanOpts {
    shadow_root: PathBuf,
    real_root: PathBuf,
    git_ref: String,
    model: String,
    force: bool,
}

pub async fn run_scan(opts: ScanOptions) -> Result<Vec<IndexEntry>> {
    let all_dirs = collect_shadow_dirs(&opts.shadow_root, &opts.scope)?;

    if all_dirs.is_empty() {
        println!("No directories to scan. Run `init` first.");
        return Ok(Vec::new());
    }

    let mut waves: BTreeMap<usize, Vec<PathBuf>> = BTreeMap::new();
    for dir in all_dirs {
        let depth = dir
            .strip_prefix(&opts.shadow_root)
            .map(|p| p.components().count())
            .unwrap_or(0);
        waves.entry(depth).or_default().push(dir);
    }

    let scan_opts = Arc::new(ScanOpts {
        shadow_root: opts.shadow_root.clone(),
        real_root: opts.real_root.clone(),
        git_ref: opts.config.git_ref.clone(),
        model: opts.model.clone(),
        force: opts.force,
    });

    let semaphore = Arc::new(Semaphore::new(opts.concurrency));
    let extractor = Arc::new(Mutex::new(SymbolExtractor::new()?));
    let mut all_entries: Vec<IndexEntry> = Vec::new();
    let mut scanned = 0usize;
    let mut skipped = 0usize;

    for (_depth, dirs) in waves.into_iter().rev() {
        let handles: Vec<_> = dirs
            .into_iter()
            .map(|shadow_dir| {
                let permit = semaphore.clone();
                let task_opts = scan_opts.clone();
                let ext = extractor.clone();
                tokio::spawn(async move {
                    let _permit = permit
                        .acquire_owned()
                        .await
                        .expect("semaphore closed");
                    scan_one_dir(shadow_dir, task_opts, ext).await
                })
            })
            .collect();

        let results = join_all(handles).await;
        for res in results {
            match res {
                Ok(Ok(ScanResult::Scanned(entries))) => {
                    scanned += 1;
                    all_entries.extend(entries);
                }
                Ok(Ok(ScanResult::Skipped)) => {
                    skipped += 1;
                }
                Ok(Err(e)) => {
                    eprintln!("warning: scan error: {e:#}");
                }
                Err(e) => {
                    eprintln!("warning: task panicked: {e}");
                }
            }
        }
    }

    println!("scan complete: {scanned} scanned, {skipped} skipped");
    Ok(all_entries)
}

enum ScanResult {
    Scanned(Vec<IndexEntry>),
    Skipped,
}

struct FileResult {
    path: String,
    oid: String,
    content: Option<FileContent>,
    symbols: Vec<IndexEntry>,
}

async fn scan_one_dir(
    shadow_dir: PathBuf,
    opts: Arc<ScanOpts>,
    extractor: Arc<Mutex<SymbolExtractor>>,
) -> Result<ScanResult> {
    let rel = shadow_dir
        .strip_prefix(&opts.shadow_root)
        .context("shadow dir not under shadow root")?;
    let real_dir_str = rel.to_string_lossy().to_string();

    let meta = read_meta(&shadow_dir)?;
    if meta.dir_hash.is_some() && !opts.force {
        let fresh_hash = compute_dir_hash(&opts.real_root, &opts.git_ref, &real_dir_str).await?;
        if Some(&fresh_hash) == meta.dir_hash.as_ref() {
            return Ok(ScanResult::Skipped);
        }
    }

    let entries = git_ls_tree(&opts.real_root, &opts.git_ref, Some(&real_dir_str)).await?;

    let mut blobs: Vec<(String, String)> = Vec::new();
    let mut child_dirs: Vec<String> = Vec::new();

    for e in &entries {
        let entry_rel = e.path.strip_prefix(&real_dir_str).unwrap_or(&e.path);
        let entry_rel = entry_rel.trim_start_matches('/');
        if entry_rel.contains('/') {
            continue;
        }
        match e.kind {
            TreeEntryKind::Blob => blobs.push((e.path.clone(), e.oid.clone())),
            TreeEntryKind::Tree => child_dirs.push(e.path.clone()),
        }
    }

    let mut file_results: Vec<FileResult> = Vec::new();

    for (blob_path, oid) in &blobs {
        let raw = match git_show(&opts.real_root, &opts.git_ref, blob_path).await {
            Ok(b) => b,
            Err(e) => {
                tracing::warn!("cannot read {blob_path}: {e}");
                continue;
            }
        };

        let path = PathBuf::from(blob_path);
        let symbols = {
            let mut ext = extractor.lock().unwrap();
            let rel_path = path
                .strip_prefix(&opts.real_root)
                .unwrap_or(&path)
                .to_string_lossy()
                .to_string();
            ext.extract_from_bytes(&path, &raw, &rel_path)
        };

        let content = std::str::from_utf8(&raw).ok().map(|text| {
            let truncated = if text.len() > MAX_FILE_BYTES {
                &text[..MAX_FILE_BYTES]
            } else {
                text
            };
            FileContent {
                path: blob_path.clone(),
                content: truncated.to_string(),
            }
        });

        file_results.push(FileResult {
            path: blob_path.clone(),
            oid: oid.clone(),
            content,
            symbols,
        });
    }

    let files_map: HashMap<String, String> = file_results
        .iter()
        .map(|r| (r.path.clone(), r.oid.clone()))
        .collect();
    let file_contents: Vec<FileContent> = file_results
        .iter()
        .filter_map(|r| r.content.clone())
        .collect();
    let index_entries: Vec<IndexEntry> = file_results
        .into_iter()
        .flat_map(|r| r.symbols)
        .collect();

    let mut children: Vec<ChildSummary> = Vec::new();
    let mut children_map: HashMap<String, Option<String>> = HashMap::new();

    for child_dir_path in &child_dirs {
        let child_name = Path::new(child_dir_path)
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| child_dir_path.clone());

        let child_shadow = opts
            .shadow_root
            .join(child_dir_path.trim_start_matches('/'));
        let child_meta = read_meta(&child_shadow).unwrap_or_default();

        children_map.insert(child_name.clone(), child_meta.dir_hash.clone());

        let child_map_path = map_path(&child_shadow);
        if child_map_path.exists() {
            if let Ok(content) = std::fs::read_to_string(&child_map_path) {
                if let Some(purpose) = extract_purpose(&content) {
                    children.push(ChildSummary {
                        name: child_name,
                        purpose,
                    });
                }
            }
        }
    }

    let summary: DirSummary = summarize(&real_dir_str, &file_contents, &children, &opts.model)
        .await
        .with_context(|| format!("summarize failed for {real_dir_str}"))?;

    let mut sorted_oids: Vec<String> = blobs.iter().map(|(_, oid)| oid.clone()).collect();
    sorted_oids.sort();
    let dir_hash = {
        let mut hasher = Sha256::new();
        for oid in &sorted_oids {
            hasher.update(oid.as_bytes());
        }
        hex::encode(hasher.finalize())
    };

    let fm = MapFrontmatter {
        dir: real_dir_str.clone(),
        status: MapStatus::Scanned,
    };
    let body = format_summary_body(&summary);
    write_map(&shadow_dir, &fm, &body)?;

    let new_meta = MapMeta {
        synced_at: Some(chrono::Utc::now()),
        dir_hash: Some(dir_hash),
        files: files_map,
        children: children_map,
    };
    write_meta(&shadow_dir, &new_meta)?;

    println!("  scanned {real_dir_str}");
    Ok(ScanResult::Scanned(index_entries))
}

async fn compute_dir_hash(real_root: &Path, git_ref: &str, dir: &str) -> Result<String> {
    let entries = git_ls_tree(real_root, git_ref, Some(dir)).await?;
    let mut oids: Vec<String> = entries
        .iter()
        .filter(|e| {
            matches!(e.kind, TreeEntryKind::Blob)
                && !e
                    .path
                    .strip_prefix(dir)
                    .unwrap_or(&e.path)
                    .trim_start_matches('/')
                    .contains('/')
        })
        .map(|e| e.oid.clone())
        .collect();
    oids.sort();
    let mut hasher = Sha256::new();
    for oid in &oids {
        hasher.update(oid.as_bytes());
    }
    Ok(hex::encode(hasher.finalize()))
}

fn collect_shadow_dirs(shadow_root: &Path, scope: &Option<PathBuf>) -> Result<Vec<PathBuf>> {
    use walkdir::WalkDir;
    let mut dirs = Vec::new();
    for entry in WalkDir::new(shadow_root)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_dir())
    {
        let path = entry.path();
        if path
            .components()
            .any(|c| c.as_os_str() == ".codescout" || c.as_os_str() == ".git")
        {
            continue;
        }
        let map = map_path(path);
        if !map.exists() {
            continue;
        }
        if let Some(scope_path) = scope {
            let full_scope = shadow_root.join(scope_path);
            if !path.starts_with(&full_scope) {
                continue;
            }
        }
        dirs.push(path.to_path_buf());
    }
    Ok(dirs)
}

fn extract_purpose(map_content: &str) -> Option<String> {
    let marker = "## Purpose";
    let idx = map_content.find(marker)?;
    let after = map_content[idx + marker.len()..]
        .trim_start_matches('\n')
        .trim_start();
    let purpose = after.lines().next()?;
    if purpose.is_empty() {
        return None;
    }
    Some(purpose.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn extract_purpose_found() {
        let content = "---\ndir: x\nstatus: scanned\n---\n## Purpose\n\nDoes cool things\n";
        assert_eq!(
            extract_purpose(content),
            Some("Does cool things".to_string())
        );
    }

    #[test]
    fn extract_purpose_missing_section() {
        let content = "---\ndir: x\nstatus: scanned\n---\n## Files\n\nsome content\n";
        assert!(extract_purpose(content).is_none());
    }

    #[test]
    fn extract_purpose_empty_after_marker() {
        let content = "## Purpose\n\n";
        assert!(extract_purpose(content).is_none());
    }

    #[test]
    fn collect_shadow_dirs_finds_dirs_with_map() {
        let dir = tempdir().unwrap();
        let sub = dir.path().join("src").join("commands");
        std::fs::create_dir_all(&sub).unwrap();
        std::fs::write(
            sub.join("_MAP.md"),
            "---\ndir: src/commands\nstatus: placeholder\n---\n",
        )
        .unwrap();
        let dirs = collect_shadow_dirs(dir.path(), &None).unwrap();
        assert_eq!(dirs.len(), 1);
        assert_eq!(dirs[0], sub);
    }

    #[test]
    fn collect_shadow_dirs_ignores_dirs_without_map() {
        let dir = tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("src").join("no_map")).unwrap();
        let dirs = collect_shadow_dirs(dir.path(), &None).unwrap();
        assert!(dirs.is_empty());
    }

    #[test]
    fn collect_shadow_dirs_skips_codescout_internals() {
        let dir = tempdir().unwrap();
        let internal = dir.path().join(".codescout");
        std::fs::create_dir_all(&internal).unwrap();
        std::fs::write(
            internal.join("_MAP.md"),
            "---\ndir: .codescout\nstatus: placeholder\n---\n",
        )
        .unwrap();
        let dirs = collect_shadow_dirs(dir.path(), &None).unwrap();
        assert!(dirs.is_empty());
    }

    #[test]
    fn collect_shadow_dirs_respects_scope() {
        let dir = tempdir().unwrap();
        let a = dir.path().join("src").join("a");
        let b = dir.path().join("src").join("b");
        std::fs::create_dir_all(&a).unwrap();
        std::fs::create_dir_all(&b).unwrap();
        std::fs::write(
            a.join("_MAP.md"),
            "---\ndir: src/a\nstatus: placeholder\n---\n",
        )
        .unwrap();
        std::fs::write(
            b.join("_MAP.md"),
            "---\ndir: src/b\nstatus: placeholder\n---\n",
        )
        .unwrap();
        let scope = Some(PathBuf::from("src/a"));
        let dirs = collect_shadow_dirs(dir.path(), &scope).unwrap();
        assert_eq!(dirs.len(), 1);
        assert_eq!(dirs[0], a);
    }
}

impl SymbolExtractor {
    pub fn extract_from_bytes(
        &mut self,
        path: &Path,
        source: &[u8],
        rel_path: &str,
    ) -> Vec<IndexEntry> {
        use crate::symbols::lang_for_path;
        const MAX_BYTES: usize = 200 * 1024;
        if source.len() > MAX_BYTES {
            return Vec::new();
        }
        let lang = match lang_for_path(path) {
            Some(l) => l,
            None => return Vec::new(),
        };
        self.extract(lang, source, rel_path)
    }
}
