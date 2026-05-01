use std::path::{Path, PathBuf};

pub const MAP_FILE: &str = "_MAP.md";
pub const META_FILE: &str = "_MAP.meta.json";
pub const INDEX_FILE: &str = "_INDEX.md";
pub const CLAUDE_MD: &str = "CLAUDE.md";
pub const CONFIG_DIR: &str = ".codescout";
pub const CONFIG_FILE: &str = ".codescout/config.json";
pub const IGNORE_FILE: &str = ".codescout/ignore";

pub fn map_path(shadow_dir: &Path) -> PathBuf {
    shadow_dir.join(MAP_FILE)
}

pub fn meta_path(shadow_dir: &Path) -> PathBuf {
    shadow_dir.join(META_FILE)
}

pub fn index_path(shadow_root: &Path) -> PathBuf {
    shadow_root.join(INDEX_FILE)
}

pub fn config_path(shadow_root: &Path) -> PathBuf {
    shadow_root.join(CONFIG_FILE)
}

pub fn ignore_path(shadow_root: &Path) -> PathBuf {
    shadow_root.join(IGNORE_FILE)
}

pub fn claude_md_path(shadow_root: &Path) -> PathBuf {
    shadow_root.join(CLAUDE_MD)
}
