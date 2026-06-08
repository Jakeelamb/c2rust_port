use anyhow::{Context, Result};
use camino::{Utf8Path, Utf8PathBuf};
use serde::Serialize;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct InitPlan {
    pub input_repo: Utf8PathBuf,
    pub source_repo: Utf8PathBuf,
    pub target_repo: Utf8PathBuf,
    pub layout: PortLayout,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub enum PortLayout {
    SeparateSourceTarget,
    VendoredSource,
}

pub fn default_target_for_source(source: &Utf8Path) -> Result<Utf8PathBuf> {
    let name = source
        .file_name()
        .filter(|s| !s.is_empty())
        .context("source path has no final component")?;
    let parent = source.parent().unwrap_or_else(|| Utf8Path::new("."));
    Ok(parent.join(format!("{name}-rs")))
}

pub fn resolve_repo_plan(input: &Utf8Path) -> Result<InitPlan> {
    if let Some(source) = detect_vendored_source(input)? {
        return Ok(InitPlan {
            input_repo: input.to_path_buf(),
            source_repo: source,
            target_repo: input.to_path_buf(),
            layout: PortLayout::VendoredSource,
        });
    }

    Ok(InitPlan {
        input_repo: input.to_path_buf(),
        source_repo: input.to_path_buf(),
        target_repo: default_target_for_source(input)?,
        layout: PortLayout::SeparateSourceTarget,
    })
}

pub fn apply_init(plan: &InitPlan) -> Result<()> {
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

fn detect_vendored_source(input: &Utf8Path) -> Result<Option<Utf8PathBuf>> {
    let upstream = input.join("reference/upstream");
    if !input.join("Cargo.toml").exists() || !upstream.is_dir() {
        return Ok(None);
    }

    let mut candidates = Vec::new();
    for entry in std::fs::read_dir(&upstream).with_context(|| format!("read {upstream}"))? {
        let entry = entry?;
        if entry.file_type()?.is_dir() {
            let path = Utf8PathBuf::from_path_buf(entry.path())
                .map_err(|p| anyhow::anyhow!("non-utf8 path: {}", p.display()))?;
            candidates.push(path);
        }
    }
    candidates.sort();
    Ok(candidates.into_iter().next())
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
    fn plain_source_uses_sibling_target() {
        let plan = resolve_repo_plan(Utf8Path::new("/tmp/bowtie2")).unwrap();
        assert_eq!(plan.source_repo, Utf8PathBuf::from("/tmp/bowtie2"));
        assert_eq!(plan.target_repo, Utf8PathBuf::from("/tmp/bowtie2-rs"));
        assert_eq!(plan.layout, PortLayout::SeparateSourceTarget);
    }

    #[test]
    fn vendored_target_is_detected_from_single_repo_arg() {
        let root = Utf8PathBuf::from_path_buf(std::env::temp_dir())
            .unwrap()
            .join(format!("c2rust-port-init-test-{}", std::process::id()));
        let source = root.join("reference/upstream/SPAdes-4.2.0");
        std::fs::create_dir_all(&source).unwrap();
        std::fs::write(root.join("Cargo.toml"), "[package]\nname = \"spades-rs\"\n").unwrap();

        let plan = resolve_repo_plan(&root).unwrap();
        assert_eq!(plan.source_repo, source);
        assert_eq!(plan.target_repo, root);
        assert_eq!(plan.layout, PortLayout::VendoredSource);

        let _ = std::fs::remove_dir_all(plan.target_repo);
    }

    #[test]
    fn init_scaffold_does_not_create_fake_binary_entrypoint() {
        let root = Utf8PathBuf::from_path_buf(std::env::temp_dir())
            .unwrap()
            .join(format!(
                "c2rust-port-init-scaffold-test-{}",
                std::process::id()
            ));
        let plan = InitPlan {
            input_repo: root.join("source"),
            source_repo: root.join("source"),
            target_repo: root.join("target"),
            layout: PortLayout::SeparateSourceTarget,
        };

        apply_init(&plan).unwrap();

        assert!(plan.target_repo.join("src/lib.rs").exists());
        assert!(!plan.target_repo.join("src/main.rs").exists());

        let _ = std::fs::remove_dir_all(root);
    }
}
