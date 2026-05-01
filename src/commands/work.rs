use crate::config::{resolve_shadow, ShadowConfig};
use crate::error::Result;
use crate::shadow::layout::claude_md_path;
use std::path::PathBuf;

pub struct WorkArgs {
    pub real: Option<PathBuf>,
    pub shadow: Option<PathBuf>,
    pub print_prompt: bool,
    pub claude_args: Vec<String>,
}

pub async fn run(args: WorkArgs) -> Result<()> {
    let shadow_root = resolve_shadow(args.shadow, args.real.as_deref())?;
    let config = ShadowConfig::load(&shadow_root)?;
    let real_root = PathBuf::from(&config.target);

    // Read CLAUDE.md as the system prompt
    let claude_md_file = claude_md_path(&shadow_root);
    let system_prompt = if claude_md_file.exists() {
        std::fs::read_to_string(&claude_md_file)?
    } else {
        crate::prompt::claude_md_content(&shadow_root, &real_root)
    };

    if args.print_prompt {
        println!("{system_prompt}");
        return Ok(());
    }

    // Build claude invocation
    // claude --add-dir <shadow> --append-system-prompt "<prompt>" [extra args...]
    let mut cmd_args: Vec<String> = Vec::new();
    cmd_args.push("--add-dir".to_string());
    cmd_args.push(shadow_root.to_string_lossy().to_string());
    cmd_args.push("--append-system-prompt".to_string());
    cmd_args.push(system_prompt);
    cmd_args.extend(args.claude_args);

    // exec-replace current process so signals pass through naturally
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        let err = std::process::Command::new("claude")
            .args(&cmd_args)
            .current_dir(&real_root)
            .exec();
        // exec only returns on error
        return Err(err.into());
    }

    #[cfg(not(unix))]
    {
        let status = tokio::process::Command::new("claude")
            .args(&cmd_args)
            .current_dir(&real_root)
            .status()
            .await?;
        std::process::exit(status.code().unwrap_or(1));
    }
}
