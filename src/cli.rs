use anyhow::Result;
use camino::Utf8PathBuf;
use clap::Parser;

use crate::{bench, init, inspect, knowledge, packets};

#[derive(Debug, Parser)]
#[command(name = "c2rust-port")]
#[command(about = "Map one C/C++ porting repo and generate bounded Rust porting packets")]
#[command(disable_help_flag = true, disable_help_subcommand = true)]
pub struct Cli {
    /// A C/C++ source repo, or a Rust port repo with reference/upstream/<source>.
    pub repo: Utf8PathBuf,
}

pub fn run(cli: Cli) -> Result<()> {
    let plan = init::resolve_repo_plan(&cli.repo)?;
    init::apply_init(&plan)?;
    inspect::run(&plan.source_repo)?;
    bench::prepare(&plan.source_repo)?;
    bench::run_source(&plan.source_repo)?;
    knowledge::run(&plan.source_repo, &plan.target_repo)?;
    packets::run(&plan.source_repo, &plan.target_repo)?;
    println!("{}", serde_json::to_string_pretty(&plan)?);
    Ok(())
}
