use anyhow::Result;
use camino::Utf8PathBuf;
use clap::{Args, Parser, Subcommand};

use crate::{bench, init, inspect, packets};

#[derive(Debug, Parser)]
#[command(name = "c2rust-port")]
#[command(about = "Map C/C++ repos and generate bounded Rust porting packets")]
pub struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    Init(InitArgs),
    Inspect(SourceArg),
    Bench(BenchArgs),
    Packets(PacketsArgs),
}

#[derive(Debug, Args)]
pub struct InitArgs {
    /// Source C/C++ repository. Positional form is kept for the v1 workflow.
    pub source_repo: Option<Utf8PathBuf>,
    #[arg(long)]
    pub source: Option<Utf8PathBuf>,
    #[arg(long)]
    pub target: Option<Utf8PathBuf>,
    /// Write files. Without this, init prints and records the planned target only.
    #[arg(long)]
    pub apply: bool,
    #[arg(long)]
    pub dry_run: bool,
}

#[derive(Debug, Args)]
pub struct SourceArg {
    pub source_repo: Utf8PathBuf,
}

#[derive(Debug, Args)]
pub struct PacketsArgs {
    pub source_repo: Utf8PathBuf,
    pub target_repo: Utf8PathBuf,
}

#[derive(Debug, Args)]
pub struct BenchArgs {
    #[command(subcommand)]
    pub command: BenchCommand,
}

#[derive(Debug, Subcommand)]
pub enum BenchCommand {
    Prepare(SourceArg),
    RunSource(SourceArg),
}

pub fn run(cli: Cli) -> Result<()> {
    match cli.command {
        Command::Init(args) => init::run(args),
        Command::Inspect(args) => inspect::run(&args.source_repo),
        Command::Bench(args) => match args.command {
            BenchCommand::Prepare(args) => bench::prepare(&args.source_repo),
            BenchCommand::RunSource(args) => bench::run_source(&args.source_repo),
        },
        Command::Packets(args) => packets::run(&args.source_repo, &args.target_repo),
    }
}
