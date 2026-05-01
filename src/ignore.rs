#![allow(dead_code)]
use crate::error::Result;
use crate::shadow::layout::ignore_path;
use std::path::Path;

const HARD_CODED: &[&str] = &[
    "node_modules/",
    "__pycache__/",
    ".DS_Store",
    "*.pyc",
    "*.pyo",
];

pub struct Matcher {
    inner: ignore::gitignore::Gitignore,
}

impl Matcher {
    pub fn is_ignored(&self, path: &str, is_dir: bool) -> bool {
        self.inner
            .matched_path_or_any_parents(path, is_dir)
            .is_ignore()
    }
}

pub fn build_matcher(shadow_root: &Path) -> Result<Matcher> {
    let mut builder = ignore::gitignore::GitignoreBuilder::new(shadow_root);
    for pattern in HARD_CODED {
        builder.add_line(None, pattern)?;
    }
    let custom_path = ignore_path(shadow_root);
    if custom_path.exists() {
        let content = std::fs::read_to_string(&custom_path)?;
        for line in content.lines() {
            let line = line.trim();
            if !line.is_empty() && !line.starts_with('#') {
                builder.add_line(None, line)?;
            }
        }
    }
    Ok(Matcher {
        inner: builder.build()?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn node_modules_dir_ignored() {
        let dir = tempdir().unwrap();
        let m = build_matcher(dir.path()).unwrap();
        assert!(m.is_ignored("node_modules", true));
    }

    #[test]
    fn pycache_dir_ignored() {
        let dir = tempdir().unwrap();
        let m = build_matcher(dir.path()).unwrap();
        assert!(m.is_ignored("__pycache__", true));
    }

    #[test]
    fn pyc_file_ignored() {
        let dir = tempdir().unwrap();
        let m = build_matcher(dir.path()).unwrap();
        assert!(m.is_ignored("foo.pyc", false));
        assert!(m.is_ignored("src/bar.pyc", false));
    }

    #[test]
    fn regular_source_file_not_ignored() {
        let dir = tempdir().unwrap();
        let m = build_matcher(dir.path()).unwrap();
        assert!(!m.is_ignored("src/main.rs", false));
        assert!(!m.is_ignored("lib/utils.py", false));
    }

    #[test]
    fn custom_ignore_file_patterns_applied() {
        let dir = tempdir().unwrap();
        let ignore_dir = dir.path().join(".codescout");
        std::fs::create_dir_all(&ignore_dir).unwrap();
        std::fs::write(ignore_dir.join("ignore"), "*.log\nbuild/\n# comment\n\n").unwrap();
        let m = build_matcher(dir.path()).unwrap();
        assert!(m.is_ignored("app.log", false));
        assert!(m.is_ignored("build", true));
        assert!(!m.is_ignored("src/main.rs", false));
    }

    #[test]
    fn custom_ignore_skips_comments_and_blank_lines() {
        let dir = tempdir().unwrap();
        let ignore_dir = dir.path().join(".codescout");
        std::fs::create_dir_all(&ignore_dir).unwrap();
        std::fs::write(ignore_dir.join("ignore"), "# just a comment\n\n").unwrap();
        let m = build_matcher(dir.path()).unwrap();
        assert!(!m.is_ignored("src/main.rs", false));
    }
}
