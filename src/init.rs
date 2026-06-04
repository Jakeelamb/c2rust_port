use anyhow::{Context, Result, bail};
use camino::{Utf8Path, Utf8PathBuf};
use serde::Serialize;

use crate::cli::InitArgs;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct InitPlan {
    pub source_repo: Utf8PathBuf,
    pub target_repo: Utf8PathBuf,
    pub apply: bool,
}

pub fn default_target_for_source(source: &Utf8Path) -> Result<Utf8PathBuf> {
    let name = source
        .file_name()
        .filter(|s| !s.is_empty())
        .context("source path has no final component")?;
    let parent = source.parent().unwrap_or_else(|| Utf8Path::new("."));
    Ok(parent.join(format!("{name}-rs")))
}

pub fn resolve_init_plan(args: &InitArgs) -> Result<InitPlan> {
    let source = args
        .source
        .clone()
        .or_else(|| args.source_repo.clone())
        .context("init requires a source path")?;
    if args.apply && args.dry_run {
        bail!("--apply and --dry-run are mutually exclusive");
    }
    let target = match &args.target {
        Some(target) => target.clone(),
        None => default_target_for_source(&source)?,
    };
    Ok(InitPlan {
        source_repo: source,
        target_repo: target,
        apply: args.apply,
    })
}

pub fn run(args: InitArgs) -> Result<()> {
    let plan = resolve_init_plan(&args)?;
    if plan.apply {
        apply_init(&plan)?;
    }
    println!("{}", serde_json::to_string_pretty(&plan)?);
    Ok(())
}

fn apply_init(plan: &InitPlan) -> Result<()> {
    std::fs::create_dir_all(plan.target_repo.join("src"))
        .with_context(|| format!("create {}", plan.target_repo))?;
    std::fs::create_dir_all(plan.target_repo.join(".c-to-rust-port/agents"))?;
    std::fs::create_dir_all(plan.target_repo.join(".c-to-rust-port/units/000-bootstrap"))?;
    std::fs::create_dir_all(plan.target_repo.join(".c-to-rust-port/prompt_profiles"))?;

    write_new(
        &plan.target_repo.join("Cargo.toml"),
        "[package]\nname = \"ported_source\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\n[dependencies]\n",
    )?;
    write_new(
        &plan.target_repo.join("src/lib.rs"),
        "pub fn placeholder() {}\n",
    )?;
    write_new(
        &plan.target_repo.join("src/main.rs"),
        "fn main() {\n    println!(\"ported source placeholder\");\n}\n",
    )?;
    write_new(
        &plan.target_repo.join("README.md"),
        "# Rust Port\n\nGenerated scaffold for a C/C++ to Rust port.\n",
    )?;
    write_new(
        &plan.target_repo.join("PORTING.md"),
        "# Porting Notes\n\nSource mapping and packet work live under `.c-to-rust-port/`.\n",
    )?;
    write_new(
        &plan.target_repo.join("GOAL.md"),
        "# Goal\n\nReach behavior parity with the source project through bounded packets.\n",
    )?;
    write_new(
        &plan.target_repo.join(".c-to-rust-port/STATUS.md"),
        "# Port Status\n\n- [ ] Source mapped\n- [ ] First packet applied\n",
    )?;
    write_new(
        &plan
            .target_repo
            .join(".c-to-rust-port/agents/translator.md"),
        "# Translator Rules\n\nDraft only. Do not run git, Cargo, build, benchmark, package-manager, broad scans, or edit the shared worktree.\n",
    )?;
    write_new(
        &plan
            .target_repo
            .join(".c-to-rust-port/units/000-bootstrap/TASK.md"),
        "# TASK: Bootstrap\n\nConfirm source inventory and propose the first narrow translation slice.\n",
    )
}

fn write_new(path: &Utf8Path, text: &str) -> Result<()> {
    if path.exists() {
        return Ok(());
    }
    std::fs::write(path, text).with_context(|| format!("write {path}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn derives_sibling_target_name() {
        assert_eq!(
            default_target_for_source(Utf8Path::new("/tmp/bowtie2")).unwrap(),
            Utf8PathBuf::from("/tmp/bowtie2-rs")
        );
    }

    #[test]
    fn explicit_vendored_target_wins() {
        let plan = resolve_init_plan(&InitArgs {
            source_repo: None,
            source: Some("spades-rs/reference/upstream/SPAdes-4.2.0".into()),
            target: Some("spades-rs".into()),
            apply: false,
            dry_run: true,
        })
        .unwrap();
        assert_eq!(plan.target_repo, Utf8PathBuf::from("spades-rs"));
    }
}
