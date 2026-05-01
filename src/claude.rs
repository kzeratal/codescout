use crate::error::{CodescoutError, Result};
use crate::prompt::{ChildSummary, FileContent};
use serde::{Deserialize, Serialize};
use tokio::io::AsyncWriteExt;
use tokio::process::Command;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DirSummary {
    pub purpose: String,
    pub deps: Vec<String>,
    pub gotchas: Vec<String>,
}

/// Response wrapper from `claude -p --output-format json`
#[derive(Debug, Deserialize)]
struct ClaudeResponse {
    result: Option<String>,
}

pub async fn summarize(
    dir: &str,
    files: &[FileContent],
    children: &[ChildSummary],
    model: &str,
) -> Result<DirSummary> {
    let prompt = crate::prompt::build_scan_prompt(dir, files, children);

    let mut child = Command::new("claude")
        .args(["-p", "--output-format", "json", "--model", model])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()?;

    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(prompt.as_bytes()).await?;
    }

    let output = child.wait_with_output().await?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        return Err(CodescoutError::ClaudeFailed(stderr).into());
    }

    let raw = String::from_utf8_lossy(&output.stdout);

    // claude -p --output-format json wraps the result in {"result": "..."}
    // Try to parse the wrapper first, then fall back to direct JSON.
    let json_text = if let Ok(wrapper) = serde_json::from_str::<ClaudeResponse>(&raw) {
        wrapper.result.unwrap_or_else(|| raw.to_string())
    } else {
        raw.to_string()
    };

    let summary: DirSummary = serde_json::from_str(json_text.trim()).map_err(|e| {
        CodescoutError::ClaudeFailed(format!(
            "failed to parse claude response as DirSummary: {e}\nraw: {json_text}"
        ))
    })?;

    Ok(summary)
}

pub fn format_summary_body(summary: &DirSummary) -> String {
    let mut lines = Vec::new();
    lines.push(format!("## Purpose\n\n{}\n", summary.purpose));
    if !summary.deps.is_empty() {
        lines.push("## Dependencies\n".to_string());
        for d in &summary.deps {
            lines.push(format!("- {d}"));
        }
        lines.push(String::new());
    }
    if !summary.gotchas.is_empty() {
        lines.push("## Gotchas\n".to_string());
        for g in &summary.gotchas {
            lines.push(format!("- {g}"));
        }
        lines.push(String::new());
    }
    lines.join("\n")
}
