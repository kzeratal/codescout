use super::init::{run as run_init, InitArgs};
use crate::config::resolve_shadow;
use crate::error::Result;
use crate::git::{git_is_repo, git_toplevel};
use std::path::PathBuf;

pub struct RebuildArgs {
    pub real_repo: PathBuf,
    pub shadow: Option<PathBuf>,
    pub git_ref: String,
}

pub async fn run(args: RebuildArgs) -> Result<()> {
    let real_repo = args
        .real_repo
        .canonicalize()
        .map_err(|e| anyhow::anyhow!("cannot access {}: {e}", args.real_repo.display()))?;

    if !git_is_repo(&real_repo).await {
        anyhow::bail!("not a git repository: {}", real_repo.display());
    }

    let real_root = git_toplevel(&real_repo).await?;
    let shadow_root = resolve_shadow(args.shadow.clone(), Some(&real_root))?;

    if shadow_root.exists() {
        println!("removing existing shadow at {}", shadow_root.display());
        std::fs::remove_dir_all(&shadow_root)?;
    }

    run_init(InitArgs {
        real_repo: args.real_repo,
        shadow: Some(shadow_root),
        git_ref: args.git_ref,
    })
    .await
}
