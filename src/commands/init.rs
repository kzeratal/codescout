use crate::config::{resolve_shadow, ShadowConfig};
use crate::error::Result;
use crate::git::{git_is_repo, git_ls_tree, git_toplevel, TreeEntryKind};
use crate::shadow::layout::{claude_md_path, ignore_path, CONFIG_DIR};
use crate::shadow::map::write_placeholder_map;
use anyhow::Context;
use std::path::{Path, PathBuf};

pub struct InitArgs {
    pub real_repo: PathBuf,
    pub shadow: Option<PathBuf>,
    pub git_ref: String,
}

pub async fn run(args: InitArgs) -> Result<()> {
    let real_repo = args
        .real_repo
        .canonicalize()
        .with_context(|| format!("cannot access repo path: {}", args.real_repo.display()))?;

    if !git_is_repo(&real_repo).await {
        anyhow::bail!("not a git repository: {}", real_repo.display());
    }

    let real_root = git_toplevel(&real_repo).await?;
    let shadow_root = resolve_shadow(args.shadow, Some(&real_root))?;

    if shadow_root.join(CONFIG_DIR).exists() {
        anyhow::bail!(
            "shadow already initialised at {}; use `rebuild` to start fresh",
            shadow_root.display()
        );
    }

    std::fs::create_dir_all(&shadow_root)?;

    let config = ShadowConfig::new(&real_root, &args.git_ref);
    config.save(&shadow_root)?;

    // Write .codescout/ignore with defaults
    let ignore_file = ignore_path(&shadow_root);
    std::fs::write(
        &ignore_file,
        "# Custom ignore patterns (gitignore syntax)\n# Example:\n# vendor/\n# *.lock\n",
    )?;

    // Write CLAUDE.md
    let claude_md = crate::prompt::claude_md_content(&shadow_root, &real_root);
    std::fs::write(claude_md_path(&shadow_root), claude_md)?;

    // Mirror directory tree from git
    let entries = git_ls_tree(&real_root, &args.git_ref, None).await?;

    let mut dir_count = 0usize;
    for entry in &entries {
        if entry.kind != TreeEntryKind::Tree {
            continue;
        }
        let shadow_dir = shadow_root.join(&entry.path);
        std::fs::create_dir_all(&shadow_dir)?;
        write_placeholder_map(&shadow_dir, &entry.path)?;
        dir_count += 1;
    }

    // Also write root _MAP.md
    write_placeholder_map(&shadow_root, "")?;

    // Write empty _INDEX.md
    write_empty_index(&shadow_root)?;

    println!(
        "initialised shadow at {}\n  real repo: {}\n  {} directories mirrored",
        shadow_root.display(),
        real_root.display(),
        dir_count,
    );
    Ok(())
}

fn write_empty_index(shadow_root: &Path) -> Result<()> {
    use crate::shadow::layout::index_path;
    let path = index_path(shadow_root);
    std::fs::write(path, "# name|kind|location\n")?;
    Ok(())
}
