use std::path::Path;

#[derive(Clone)]
pub struct FileContent {
    pub path: String,
    pub content: String,
}

pub struct ChildSummary {
    pub name: String,
    pub purpose: String,
}

pub fn build_scan_prompt(dir: &str, files: &[FileContent], children: &[ChildSummary]) -> String {
    let mut parts = Vec::new();
    parts.push(format!(
        "You are summarizing the source directory `{dir}` for a codebase navigation index.\n\
         Respond with ONLY a JSON object — no markdown fences, no explanation:\n\
         {{\"purpose\": \"one sentence\", \"deps\": [\"...\"], \"gotchas\": [\"...\"]}}\n\
         Use plain text, no bold/italic. Keep each field concise."
    ));

    if !files.is_empty() {
        parts.push("\n## Files\n".to_string());
        for f in files {
            parts.push(format!("--- {} ---\n{}", f.path, f.content));
        }
    }

    if !children.is_empty() {
        parts.push("\n## Child directories\n".to_string());
        for c in children {
            parts.push(format!("{}: {}", c.name, c.purpose));
        }
    }

    parts.join("\n")
}

pub fn claude_md_content(shadow_root: &Path, real_repo: &Path) -> String {
    format!(
        "# Codescout Navigation Map\n\n\
         This directory mirrors the structure of `{real}` with AI-generated semantic summaries.\n\n\
         ## How to use\n\n\
         - Each directory contains a `_MAP.md` with a one-sentence purpose, dependency list, and gotchas.\n\
         - `_INDEX.md` at the root is a flat `name|kind|location` symbol index — grep it to locate any symbol in O(file size).\n\
         - Read `_MAP.md` in a directory before diving into its source files.\n\
         - Use `_INDEX.md` to find where a specific function, class, or type is defined.\n\n\
         Shadow root: `{shadow}`\n\
         Real repo:   `{real}`\n",
        real = real_repo.display(),
        shadow = shadow_root.display(),
    )
}
