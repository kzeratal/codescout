use clap::{Args, Parser, Subcommand};
use clap_complete::Shell;
use std::path::PathBuf;

#[derive(Debug, Parser)]
#[command(
    name = "codescout",
    version,
    about = "Shadow repository generator for AI-assisted codebase navigation"
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Mirror a git repo's directory tree into a shadow with placeholder _MAP.md files
    Init(InitArgs),
    /// Wipe the shadow and re-initialise from scratch
    Rebuild(RebuildArgs),
    /// Walk the shadow bottom-up and summarise each directory with Claude
    Scan(ScanArgs),
    /// Detect stale/new/orphaned directories and refresh the shadow
    Sync(SyncArgs),
    /// Show PLACEHOLDER/FRESH/STALE status for each shadow directory
    Status(StatusArgs),
    /// Spawn claude CLI in the real repo with the shadow injected
    Work(WorkArgs),
    /// Print shell completion script
    Completion(CompletionArgs),
    /// Print version
    Version,
}

#[derive(Debug, Args)]
pub struct InitArgs {
    /// Path to the real git repository
    pub real_repo: PathBuf,
    /// Override the shadow directory (default: ~/.codescout/projects/<repo-name>)
    #[arg(long)]
    pub shadow: Option<PathBuf>,
    /// Git ref to mirror (default: HEAD)
    #[arg(long, default_value = "HEAD")]
    pub git_ref: String,
}

#[derive(Debug, Args)]
pub struct RebuildArgs {
    /// Path to the real git repository
    pub real_repo: PathBuf,
    #[arg(long)]
    pub shadow: Option<PathBuf>,
    #[arg(long, default_value = "HEAD")]
    pub git_ref: String,
}

#[derive(Debug, Args)]
pub struct ScanArgs {
    #[arg(long)]
    pub shadow: Option<PathBuf>,
    /// Limit scan to this subtree (repo-relative path)
    #[arg(long)]
    pub scope: Option<PathBuf>,
    /// Number of parallel workers
    #[arg(long, default_value = "8")]
    pub concurrency: usize,
    /// Claude model to use for summarisation
    #[arg(long, default_value = "claude-haiku-4-5")]
    pub model: String,
    /// Force rescan of already-scanned directories
    #[arg(long)]
    pub force: bool,
}

#[derive(Debug, Args)]
pub struct SyncArgs {
    #[arg(long)]
    pub shadow: Option<PathBuf>,
    #[arg(long)]
    pub scope: Option<PathBuf>,
}

#[derive(Debug, Args)]
pub struct StatusArgs {
    #[arg(long)]
    pub shadow: Option<PathBuf>,
    #[arg(long)]
    pub scope: Option<PathBuf>,
}

#[derive(Debug, Args)]
pub struct WorkArgs {
    /// Path to the real repository (optional if inside one)
    #[arg(long)]
    pub real: Option<PathBuf>,
    #[arg(long)]
    pub shadow: Option<PathBuf>,
    /// Print the system prompt without spawning claude
    #[arg(long)]
    pub print_prompt: bool,
    /// Additional arguments passed directly to claude
    #[arg(last = true)]
    pub claude_args: Vec<String>,
}

#[derive(Debug, Args)]
pub struct CompletionArgs {
    /// Shell to generate completions for
    pub shell: Shell,
}
