use crate::config::{resolve_shadow, ShadowConfig};
use crate::error::Result;
use crate::scanner::{run_scan, ScanOptions};
use crate::shadow::index::write_index;
use std::path::PathBuf;

pub struct ScanArgs {
    pub shadow: Option<PathBuf>,
    pub scope: Option<PathBuf>,
    pub concurrency: usize,
    pub model: String,
    pub force: bool,
}

pub async fn run(args: ScanArgs) -> Result<()> {
    let shadow_root = resolve_shadow(args.shadow, None)?;
    let config = ShadowConfig::load(&shadow_root)?;
    let real_root = PathBuf::from(&config.target);

    let entries = run_scan(ScanOptions {
        shadow_root: shadow_root.clone(),
        real_root,
        config,
        scope: args.scope,
        concurrency: args.concurrency,
        model: args.model,
        force: args.force,
    })
    .await?;

    write_index(&shadow_root, &entries)?;
    println!("wrote _INDEX.md with {} entries", entries.len());
    Ok(())
}
