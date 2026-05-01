mod claude;
mod cli;
mod commands;
mod config;
mod error;
mod git;
mod ignore;
mod prompt;
mod scanner;
mod shadow;
mod symbols;

use clap::Parser;
use cli::{Cli, Command};

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive(tracing::Level::WARN.into()),
        )
        .with_writer(std::io::stderr)
        .init();

    let cli = Cli::parse();

    let result = dispatch(cli).await;
    if let Err(e) = result {
        eprintln!("error: {e:#}");
        std::process::exit(1);
    }
}

async fn dispatch(cli: Cli) -> error::Result<()> {
    match cli.command {
        Command::Init(args) => {
            commands::init::run(commands::init::InitArgs {
                real_repo: args.real_repo,
                shadow: args.shadow,
                git_ref: args.git_ref,
            })
            .await
        }
        Command::Rebuild(args) => {
            commands::rebuild::run(commands::rebuild::RebuildArgs {
                real_repo: args.real_repo,
                shadow: args.shadow,
                git_ref: args.git_ref,
            })
            .await
        }
        Command::Scan(args) => {
            commands::scan::run(commands::scan::ScanArgs {
                shadow: args.shadow,
                scope: args.scope,
                concurrency: args.concurrency,
                model: args.model,
                force: args.force,
            })
            .await
        }
        Command::Sync(args) => {
            commands::sync::run(commands::sync::SyncArgs {
                shadow: args.shadow,
                scope: args.scope,
            })
            .await
        }
        Command::Status(args) => {
            commands::status::run(commands::status::StatusArgs {
                shadow: args.shadow,
                scope: args.scope,
            })
            .await
        }
        Command::Work(args) => {
            commands::work::run(commands::work::WorkArgs {
                real: args.real,
                shadow: args.shadow,
                print_prompt: args.print_prompt,
                claude_args: args.claude_args,
            })
            .await
        }
        Command::Completion(args) => commands::completion::run(args.shell),
        Command::Version => {
            println!("codescout {}", env!("CARGO_PKG_VERSION"));
            Ok(())
        }
    }
}
