use crate::config::{resolve_shadow, ShadowConfig};
use crate::error::Result;
use crate::git::{git_ls_tree, TreeEntryKind};
use crate::shadow::layout::map_path;
use crate::shadow::map::{read_meta, write_placeholder_map};
use std::collections::HashSet;
use std::path::PathBuf;
use walkdir::WalkDir;

pub struct SyncArgs {
    pub shadow: Option<PathBuf>,
    pub scope: Option<PathBuf>,
}

pub async fn run(args: SyncArgs) -> Result<()> {
    let shadow_root = resolve_shadow(args.shadow, None)?;
    let config = ShadowConfig::load(&shadow_root)?;
    let real_root = PathBuf::from(&config.target);

    let scope_str = args.scope.as_ref().map(|p| p.to_string_lossy().to_string());

    // Get current real repo directories from git
    let entries = git_ls_tree(&real_root, &config.git_ref, scope_str.as_deref()).await?;
    let real_dirs: HashSet<String> = entries
        .iter()
        .filter(|e| e.kind == TreeEntryKind::Tree)
        .map(|e| e.path.clone())
        .collect();

    let mut new_count = 0usize;
    let mut orphan_count = 0usize;
    let mut stale_count = 0usize;

    // Walk shadow dirs
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

    for shadow_dir in &shadow_dirs {
        let rel = match shadow_dir.strip_prefix(&shadow_root) {
            Ok(r) => r.to_string_lossy().to_string(),
            Err(_) => continue,
        };
        if rel.is_empty() {
            continue; // root dir
        }

        if !real_dirs.contains(&rel) {
            // Orphaned — delete
            std::fs::remove_dir_all(shadow_dir)?;
            println!("  removed orphan: {rel}");
            orphan_count += 1;
            continue;
        }

        // Check staleness via dir_hash
        let meta = read_meta(shadow_dir)?;
        if let Some(stored_hash) = &meta.dir_hash {
            let fresh_hash = compute_dir_hash(&real_root, &config.git_ref, &rel).await?;
            if *stored_hash != fresh_hash {
                // Mark stale by resetting to placeholder
                write_placeholder_map(shadow_dir, &rel)?;
                let mut new_meta = meta;
                new_meta.dir_hash = None;
                new_meta.synced_at = None;
                crate::shadow::map::write_meta(shadow_dir, &new_meta)?;
                println!("  stale: {rel}");
                stale_count += 1;
            }
        }
    }

    // Add new dirs from real repo not in shadow
    let shadow_rels: HashSet<String> = shadow_dirs
        .iter()
        .filter_map(|d| {
            d.strip_prefix(&shadow_root)
                .ok()
                .map(|r| r.to_string_lossy().to_string())
        })
        .collect();

    for real_dir in &real_dirs {
        if scope_str
            .as_ref()
            .map_or(false, |s| !real_dir.starts_with(s.as_str()))
        {
            continue;
        }
        if !shadow_rels.contains(real_dir) {
            let shadow_dir = shadow_root.join(real_dir);
            std::fs::create_dir_all(&shadow_dir)?;
            write_placeholder_map(&shadow_dir, real_dir)?;
            println!("  new: {real_dir}");
            new_count += 1;
        }
    }

    println!("sync complete: {new_count} new, {stale_count} stale, {orphan_count} orphaned");
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
