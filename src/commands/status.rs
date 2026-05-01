use crate::config::{resolve_shadow, ShadowConfig};
use crate::error::Result;
use crate::git::{git_ls_tree, TreeEntryKind};
use crate::shadow::layout::map_path;
use crate::shadow::map::{read_map, read_meta, MapStatus};
use std::path::PathBuf;
use walkdir::WalkDir;

pub struct StatusArgs {
    pub shadow: Option<PathBuf>,
    pub scope: Option<PathBuf>,
}

pub async fn run(args: StatusArgs) -> Result<()> {
    let shadow_root = resolve_shadow(args.shadow, None)?;
    let config = ShadowConfig::load(&shadow_root)?;
    let real_root = PathBuf::from(&config.target);

    let scope_str = args.scope.as_ref().map(|p| p.to_string_lossy().to_string());

    // Collect shadow dirs
    let shadow_dirs: Vec<PathBuf> = WalkDir::new(&shadow_root)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_dir())
        .filter(|e| {
            !e.path()
                .components()
                .any(|c| c.as_os_str() == ".codescout" || c.as_os_str() == ".git")
        })
        .filter(|e| map_path(e.path()).exists())
        .map(|e| e.path().to_path_buf())
        .collect();

    println!("{:<60} {}", "DIRECTORY", "STATUS");
    println!("{}", "-".repeat(75));

    for shadow_dir in &shadow_dirs {
        let rel = match shadow_dir.strip_prefix(&shadow_root) {
            Ok(r) => r.to_string_lossy().to_string(),
            Err(_) => continue,
        };

        if let Some(scope) = &scope_str {
            if !rel.starts_with(scope.as_str()) && !rel.is_empty() {
                continue;
            }
        }

        let display = if rel.is_empty() { "." } else { &rel };

        let (fm, _body) = match read_map(shadow_dir) {
            Ok(r) => r,
            Err(_) => {
                println!("{display:<60} ERROR");
                continue;
            }
        };

        let status_label = match fm.status {
            MapStatus::Placeholder => "PLACEHOLDER".to_string(),
            MapStatus::Scanned => {
                // Check freshness
                let meta = read_meta(shadow_dir).unwrap_or_default();
                if let Some(stored_hash) = &meta.dir_hash {
                    match compute_dir_hash(&real_root, &config.git_ref, &rel).await {
                        Ok(fresh) if fresh == *stored_hash => {
                            let synced = meta
                                .synced_at
                                .map(|t| t.format("%Y-%m-%d %H:%M").to_string())
                                .unwrap_or_else(|| "?".to_string());
                            format!("FRESH ({synced})")
                        }
                        _ => "STALE".to_string(),
                    }
                } else {
                    "STALE".to_string()
                }
            }
        };

        println!("{display:<60} {status_label}");
    }

    Ok(())
}

async fn compute_dir_hash(real_root: &std::path::Path, git_ref: &str, dir: &str) -> Result<String> {
    use sha2::{Digest, Sha256};
    let entries = git_ls_tree(real_root, git_ref, Some(dir)).await?;
    let mut oids: Vec<String> = entries
        .iter()
        .filter(|e| {
            e.kind == TreeEntryKind::Blob
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
