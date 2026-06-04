use anyhow::{Context, Result};
use camino::{Utf8Path, Utf8PathBuf};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::io::BufRead;

#[derive(Debug, Clone)]
struct Synthesis {
    source: Utf8PathBuf,
    target: Utf8PathBuf,
    counts: BTreeMap<String, usize>,
    tool_outcomes: Vec<ToolOutcome>,
    build_targets: Vec<BuildTarget>,
    subsystems: Vec<Subsystem>,
    pipeline: Vec<PipelineStep>,
    port_phases: Vec<PortPhase>,
}

#[derive(Debug, Clone)]
struct ToolOutcome {
    tool: String,
    status: String,
    notes: String,
}

#[derive(Debug, Clone)]
struct BuildTarget {
    name: String,
    directive: String,
    build_file: String,
    role: String,
}

#[derive(Debug, Clone)]
struct Subsystem {
    rust_path: String,
    role: String,
    purpose: String,
    source_count: usize,
    examples: Vec<String>,
    priority: u8,
}

#[derive(Debug, Clone)]
struct PipelineStep {
    name: String,
    evidence: String,
    rust_modules: Vec<String>,
}

#[derive(Debug, Clone)]
struct PortPhase {
    name: String,
    goal: String,
    modules: Vec<String>,
    verification: String,
}

pub fn run(source: &Utf8Path, target: &Utf8Path) -> Result<()> {
    let root = source.join(".c2rust-port/knowledge");
    let synthesis = synthesize(source, target, &root)?;
    write_outputs(&root, target, &synthesis)?;
    Ok(())
}

fn synthesize(source: &Utf8Path, target: &Utf8Path, root: &Utf8Path) -> Result<Synthesis> {
    let counts = fact_counts(root)?;
    let tool_outcomes = read_tool_outcomes(&root.join("raw/evidence-runs.jsonl"))?;
    let build_targets = read_build_targets(&root.join("facts/build_units.jsonl"))?;
    let subsystems = read_subsystems(&root.join("facts/repo_map.jsonl"), target)?;
    let pipeline = infer_pipeline(&subsystems);
    let port_phases = infer_port_phases(&subsystems);
    Ok(Synthesis {
        source: source.to_path_buf(),
        target: target.to_path_buf(),
        counts,
        tool_outcomes,
        build_targets,
        subsystems,
        pipeline,
        port_phases,
    })
}

fn write_outputs(root: &Utf8Path, target: &Utf8Path, synthesis: &Synthesis) -> Result<()> {
    let synthesis_dir = root.join("synthesis");
    let bundle_dir = root.join("bundles");
    let target_dir = target.join(".c-to-rust-port");
    std::fs::create_dir_all(&synthesis_dir).with_context(|| format!("create {synthesis_dir}"))?;
    std::fs::create_dir_all(&bundle_dir).with_context(|| format!("create {bundle_dir}"))?;
    std::fs::create_dir_all(&target_dir).with_context(|| format!("create {target_dir}"))?;

    let full = render_full_picture(synthesis);
    let architecture = render_architecture(synthesis);
    let subsystems = render_subsystems(synthesis);
    let build_targets = render_build_targets(synthesis);
    let pipeline = render_runtime_pipeline(synthesis);
    let port_plan = render_port_plan(synthesis);
    let context = render_porting_context(synthesis);

    write_all(&bundle_dir.join("full-picture.md"), &full)?;
    write_all(&synthesis_dir.join("ARCHITECTURE.md"), &architecture)?;
    write_all(&synthesis_dir.join("SUBSYSTEMS.md"), &subsystems)?;
    write_all(&synthesis_dir.join("BUILD_TARGETS.md"), &build_targets)?;
    write_all(&synthesis_dir.join("RUNTIME_PIPELINE.md"), &pipeline)?;
    write_all(&synthesis_dir.join("PORT_PLAN.md"), &port_plan)?;

    write_all(&target_dir.join("ARCHITECTURE.md"), &architecture)?;
    write_all(&target_dir.join("SUBSYSTEMS.md"), &subsystems)?;
    write_all(&target_dir.join("BUILD_TARGETS.md"), &build_targets)?;
    write_all(&target_dir.join("RUNTIME_PIPELINE.md"), &pipeline)?;
    write_all(&target_dir.join("PORT_PLAN.md"), &port_plan)?;
    write_all(&target_dir.join("PORTING_CONTEXT.md"), &context)?;
    Ok(())
}

fn render_full_picture(s: &Synthesis) -> String {
    let mut text = String::new();
    text.push_str("# Full Repo Picture\n\n");
    text.push_str("## Porting Intelligence Summary\n\n");
    text.push_str(&format!("- Source: `{}`\n", s.source));
    text.push_str(&format!("- Target: `{}`\n", s.target));
    text.push_str(&format!(
        "- Fact base: {} normalized facts across {} tables.\n",
        s.counts.values().sum::<usize>(),
        s.counts.len()
    ));
    text.push_str(&format!(
        "- Build understanding: {} promoted build targets/units.\n",
        s.build_targets.len()
    ));
    text.push_str(&format!(
        "- Architecture understanding: {} source ownership subsystems.\n",
        s.subsystems.len()
    ));
    text.push_str("- This file is the curated map; raw evidence remains under `.c2rust-port/knowledge/raw/` and normalized facts under `facts/`.\n\n");

    text.push_str("## What To Build In Rust\n\n");
    text.push_str("Start with behavior-preserving Rust modules that mirror source ownership, not one file per C++ header. The important initial boundary is pipeline/data ownership: reads and config, sequence primitives, k-mers, graph core, graph algorithms, project pipeline orchestration, then output/parity.\n\n");

    text.push_str("## Likely Runtime Pipeline\n\n");
    for (index, step) in s.pipeline.iter().enumerate() {
        text.push_str(&format!(
            "{}. {} - evidence: {}. Rust modules: {}.\n",
            index + 1,
            step.name,
            step.evidence,
            joined_or_dash(&step.rust_modules)
        ));
    }
    text.push('\n');

    text.push_str("## Highest Priority Port Surfaces\n\n");
    for subsystem in s.subsystems.iter().filter(|sub| sub.priority <= 2).take(24) {
        text.push_str(&format!(
            "- `{}`: {}. {} files. Purpose: {} Examples: {}.\n",
            subsystem.rust_path,
            subsystem.role,
            subsystem.source_count,
            subsystem.purpose,
            joined_or_dash(&subsystem.examples)
        ));
    }
    text.push('\n');

    text.push_str("## Build Targets And Executables\n\n");
    let promoted_targets = s
        .build_targets
        .iter()
        .filter(|target| {
            !target.role.starts_with("generated")
                && !target.role.starts_with("vendor")
                && !target.role.starts_with("test")
                && !target.role.starts_with("build relationship")
                && !target.role.starts_with("build subtree")
        })
        .collect::<Vec<_>>();
    let build_target_preview = if promoted_targets.is_empty() {
        s.build_targets.iter().collect::<Vec<_>>()
    } else {
        promoted_targets
    };
    for target in build_target_preview.iter().take(40) {
        text.push_str(&format!(
            "- `{}` ({}) from `{}`: {}\n",
            target.name, target.directive, target.build_file, target.role
        ));
    }
    if build_target_preview.len() > 40 {
        text.push_str(&format!(
            "- ... {} more build targets in `BUILD_TARGETS.md`.\n",
            build_target_preview.len() - 40
        ));
    }
    text.push('\n');

    text.push_str("## Suggested Port Plan\n\n");
    for phase in &s.port_phases {
        text.push_str(&format!(
            "- `{}`: {} Modules: {}. Verification: {}\n",
            phase.name,
            phase.goal,
            joined_or_dash(&phase.modules),
            phase.verification
        ));
    }
    text.push('\n');

    text.push_str("## Tool Outcomes\n\n");
    for outcome in &s.tool_outcomes {
        text.push_str(&format!(
            "- `{}`: {} - {}\n",
            outcome.tool, outcome.status, outcome.notes
        ));
    }
    text.push('\n');

    text.push_str("## Fact Tables\n\n");
    for (name, count) in &s.counts {
        text.push_str(&format!("- `facts/{name}.jsonl`: {count} rows\n"));
    }
    text.push_str("\n## Generated Synthesis Docs\n\n");
    text.push_str("- `synthesis/ARCHITECTURE.md`\n");
    text.push_str("- `synthesis/SUBSYSTEMS.md`\n");
    text.push_str("- `synthesis/BUILD_TARGETS.md`\n");
    text.push_str("- `synthesis/RUNTIME_PIPELINE.md`\n");
    text.push_str("- `synthesis/PORT_PLAN.md`\n");
    text
}

fn render_architecture(s: &Synthesis) -> String {
    let mut text = String::new();
    text.push_str("# Architecture\n\n");
    text.push_str("## System Shape\n\n");
    text.push_str("The source tree is organized around shared `common` infrastructure, project-specific executable pipelines under `src/projects`, bundled vendor libraries under `ext`, generated build configuration under `build_spades`, and tests under `src/test`.\n\n");
    text.push_str("## Core Runtime Ownership\n\n");
    for subsystem in s
        .subsystems
        .iter()
        .filter(|sub| !sub.role.starts_with("vendor") && !sub.role.starts_with("test"))
    {
        text.push_str(&format!(
            "- `{}`: {}. Purpose: {} Source files: {}.\n",
            subsystem.rust_path, subsystem.role, subsystem.purpose, subsystem.source_count
        ));
    }
    text
}

fn render_subsystems(s: &Synthesis) -> String {
    let mut text = String::new();
    text.push_str("# Subsystems\n\n");
    text.push_str("| Priority | Rust Module | Role | Source Files | Purpose | Examples |\n");
    text.push_str("|---:|---|---|---:|---|---|\n");
    for subsystem in &s.subsystems {
        text.push_str(&format!(
            "| {} | `{}` | {} | {} | {} | {} |\n",
            subsystem.priority,
            subsystem.rust_path,
            markdown_cell(&subsystem.role),
            subsystem.source_count,
            markdown_cell(&subsystem.purpose),
            markdown_cell(&joined_or_dash(&subsystem.examples))
        ));
    }
    text
}

fn render_build_targets(s: &Synthesis) -> String {
    let mut text = String::new();
    text.push_str("# Build Targets\n\n");
    text.push_str("| Target | Directive | Build File | Role |\n");
    text.push_str("|---|---|---|---|\n");
    for target in &s.build_targets {
        text.push_str(&format!(
            "| `{}` | `{}` | `{}` | {} |\n",
            markdown_cell(&target.name),
            target.directive,
            target.build_file,
            markdown_cell(&target.role)
        ));
    }
    text
}

fn render_runtime_pipeline(s: &Synthesis) -> String {
    let mut text = String::new();
    text.push_str("# Runtime Pipeline\n\n");
    for (index, step) in s.pipeline.iter().enumerate() {
        text.push_str(&format!("## {}. {}\n\n", index + 1, step.name));
        text.push_str(&format!("- Evidence: {}\n", step.evidence));
        text.push_str(&format!(
            "- Rust modules: {}\n\n",
            joined_or_dash(&step.rust_modules)
        ));
    }
    text
}

fn render_port_plan(s: &Synthesis) -> String {
    let mut text = String::new();
    text.push_str("# Port Plan\n\n");
    for phase in &s.port_phases {
        text.push_str(&format!("## {}\n\n", phase.name));
        text.push_str(&format!("- Goal: {}\n", phase.goal));
        text.push_str(&format!("- Modules: {}\n", joined_or_dash(&phase.modules)));
        text.push_str(&format!("- Verification: {}\n\n", phase.verification));
    }
    text
}

fn render_porting_context(s: &Synthesis) -> String {
    let mut text = String::new();
    text.push_str("# Porting Context\n\n");
    text.push_str("Read this before generating or applying translation packets.\n\n");
    text.push_str("## Rules\n\n");
    text.push_str("- Preserve behavior before refactoring architecture.\n");
    text.push_str("- Treat vendor libraries as dependencies or compatibility shims unless explicitly porting them.\n");
    text.push_str("- Port source ownership clusters, not isolated files, when behavior crosses headers and templates.\n");
    text.push_str("- Use benchmark and source-output evidence as the acceptance gate.\n\n");
    text.push_str("## First Phases\n\n");
    for phase in s.port_phases.iter().take(4) {
        text.push_str(&format!("- `{}`: {}\n", phase.name, phase.goal));
    }
    text
}

fn infer_pipeline(subsystems: &[Subsystem]) -> Vec<PipelineStep> {
    let mut steps = Vec::new();
    push_step(
        &mut steps,
        "CLI/configuration and pipeline setup",
        "project entrypoints, config structs, and pipeline modules",
        subsystems,
        &[
            "projects/spades.rs",
            "common/configs.rs",
            "common/pipeline.rs",
        ],
    );
    push_step(
        &mut steps,
        "Read/library ingestion",
        "IO, library, reads, FASTA/FASTQ, BAM/SAM source clusters",
        subsystems,
        &["common/io.rs", "common/library.rs", "common/sequence.rs"],
    );
    push_step(
        &mut steps,
        "Read correction",
        "Hammer/IonHammer/corrector project clusters",
        subsystems,
        &[
            "projects/hammer.rs",
            "projects/ionhammer.rs",
            "projects/corrector.rs",
        ],
    );
    push_step(
        &mut steps,
        "K-mer indexing and counting",
        "kmer_index and k-mer project/tool clusters",
        subsystems,
        &["common/kmer_index.rs", "projects/spades_tools.rs"],
    );
    push_step(
        &mut steps,
        "Assembly graph construction",
        "assembly_graph, construction, graph core, and sequence modules",
        subsystems,
        &[
            "common/assembly_graph.rs",
            "common/stages.rs",
            "common/modules.rs",
        ],
    );
    push_step(
        &mut steps,
        "Graph simplification and repeat/scaffold handling",
        "simplification, path extension, paired-info, and spades pipeline clusters",
        subsystems,
        &[
            "common/paired_info.rs",
            "common/modules.rs",
            "projects/spades.rs",
        ],
    );
    push_step(
        &mut steps,
        "Output, visualization, and tool programs",
        "graph/read output, visualization, and command-line tools",
        subsystems,
        &[
            "common/io.rs",
            "common/visualization.rs",
            "projects/spades_tools.rs",
        ],
    );
    steps
}

fn push_step(
    steps: &mut Vec<PipelineStep>,
    name: &str,
    evidence: &str,
    subsystems: &[Subsystem],
    needles: &[&str],
) {
    let modules = subsystems
        .iter()
        .filter(|sub| needles.iter().any(|needle| sub.rust_path.contains(needle)))
        .map(|sub| sub.rust_path.clone())
        .take(8)
        .collect::<Vec<_>>();
    steps.push(PipelineStep {
        name: name.to_string(),
        evidence: evidence.to_string(),
        rust_modules: modules,
    });
}

fn infer_port_phases(subsystems: &[Subsystem]) -> Vec<PortPhase> {
    vec![
        phase(
            "Phase 1 - Rust crate skeleton and config model",
            "Create public Rust APIs, CLI/config parsing, error types, and pipeline state containers.",
            modules_matching(
                subsystems,
                &[
                    "common/configs.rs",
                    "common/pipeline.rs",
                    "projects/spades.rs",
                ],
            ),
            "cargo test plus source command shape parity.",
        ),
        phase(
            "Phase 2 - Sequence, reads, and library IO",
            "Port nucleotide/read/quality primitives and streaming FASTA/FASTQ/library ingestion.",
            modules_matching(
                subsystems,
                &["common/sequence.rs", "common/io.rs", "common/library.rs"],
            ),
            "Round-trip fixture reads and compare source parser summaries.",
        ),
        phase(
            "Phase 3 - K-mer and graph core",
            "Port k-mer representation, indexing traits, graph storage, coverage, and traversal primitives.",
            modules_matching(
                subsystems,
                &["common/kmer_index.rs", "common/assembly_graph.rs"],
            ),
            "Micro-fixture graph/k-mer parity tests.",
        ),
        phase(
            "Phase 4 - Correction and assembly stages",
            "Port read correction, graph construction, simplification, and paired-info algorithms by stage.",
            modules_matching(
                subsystems,
                &[
                    "projects/hammer.rs",
                    "projects/ionhammer.rs",
                    "common/stages.rs",
                    "common/modules.rs",
                    "common/paired_info.rs",
                ],
            ),
            "Stage-level source/Rust output diffs on tiny benchmark subsets.",
        ),
        phase(
            "Phase 5 - Project executables and output parity",
            "Wire project pipelines and output formats, then converge on benchmark corpus parity.",
            modules_matching(
                subsystems,
                &[
                    "projects/spades.rs",
                    "projects/spades_tools.rs",
                    "common/visualization.rs",
                ],
            ),
            "End-to-end tiny/smoke/medium benchmark comparisons.",
        ),
    ]
}

fn phase(name: &str, goal: &str, modules: Vec<String>, verification: &str) -> PortPhase {
    PortPhase {
        name: name.to_string(),
        goal: goal.to_string(),
        modules,
        verification: verification.to_string(),
    }
}

fn modules_matching(subsystems: &[Subsystem], needles: &[&str]) -> Vec<String> {
    subsystems
        .iter()
        .filter(|sub| needles.iter().any(|needle| sub.rust_path.contains(needle)))
        .map(|sub| sub.rust_path.clone())
        .take(12)
        .collect()
}

fn read_subsystems(path: &Utf8Path, target: &Utf8Path) -> Result<Vec<Subsystem>> {
    let mut subsystems = Vec::new();
    for row in read_jsonl(path)? {
        if row.get("fact_type").and_then(Value::as_str) != Some("rust_mirror_module") {
            continue;
        }
        let rust_path = row
            .get("rust_path")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        let rust_path = strip_path_prefix(&rust_path, target);
        let mirrors = row
            .get("mirrors")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .map(ToString::to_string)
            .collect::<Vec<_>>();
        let role = classify_role(&rust_path, &mirrors);
        let purpose = infer_purpose(&rust_path, &mirrors);
        let priority = priority_for(&role, &rust_path);
        subsystems.push(Subsystem {
            rust_path,
            role,
            purpose,
            source_count: mirrors.len(),
            examples: mirrors.into_iter().take(4).collect(),
            priority,
        });
    }
    subsystems.sort_by(|left, right| {
        (left.priority, &left.role, &left.rust_path).cmp(&(
            right.priority,
            &right.role,
            &right.rust_path,
        ))
    });
    Ok(subsystems)
}

fn classify_role(rust_path: &str, mirrors: &[String]) -> String {
    let path = rust_path.to_lowercase();
    let evidence = mirrors.join(" ").to_lowercase();
    if path.contains("/generated/") || path.starts_with("src/generated/") {
        return "generated build configuration".to_string();
    }
    if path.contains("/vendor/") || path.starts_with("src/vendor/") {
        return "vendor dependency surface".to_string();
    }
    if path.contains("/tests/") || path.starts_with("src/tests/") {
        return "test/parity evidence".to_string();
    }
    if evidence
        .split_whitespace()
        .any(|item| item.starts_with("build_spades/"))
    {
        return "generated build configuration".to_string();
    }
    if evidence
        .split_whitespace()
        .any(|item| item.starts_with("ext/"))
    {
        return "vendor dependency surface".to_string();
    }
    if evidence
        .split_whitespace()
        .any(|item| item.starts_with("src/test/"))
    {
        return "test/parity evidence".to_string();
    }
    if path.contains("common/assembly_graph") {
        return "core assembly graph model".to_string();
    }
    if path.contains("common/kmer_index") {
        return "k-mer indexing/counting".to_string();
    }
    if path.contains("common/sequence") {
        return "sequence/read primitive model".to_string();
    }
    if path.contains("common/io") {
        return "read and graph IO".to_string();
    }
    if path.contains("common/library") {
        return "read/library metadata".to_string();
    }
    if path.contains("common/paired_info") {
        return "paired information/scaffolding".to_string();
    }
    if path.contains("common/modules") {
        return "graph algorithms and stage modules".to_string();
    }
    if path.contains("common/stages") {
        return "assembly stage orchestration".to_string();
    }
    if path.contains("common/pipeline") {
        return "pipeline data/state".to_string();
    }
    if path.contains("common/configs") {
        return "configuration model".to_string();
    }
    if path.contains("projects/hammer")
        || path.contains("projects/ionhammer")
        || path.contains("projects/corrector")
    {
        return "read correction".to_string();
    }
    if path.contains("projects/spades_tools") {
        return "support tools/output utilities".to_string();
    }
    if path.contains("projects/hpcspades") {
        return "distributed assembler variant".to_string();
    }
    if path.contains("projects/spades") {
        return "primary assembler pipeline".to_string();
    }
    if path.contains("common/visualization") {
        return "visualization/output tooling".to_string();
    }
    if path.contains("common/") {
        return "shared runtime support".to_string();
    }
    if path.contains("projects/") {
        return "project executable/tool".to_string();
    }
    "supporting source cluster".to_string()
}

fn infer_purpose(rust_path: &str, mirrors: &[String]) -> String {
    let role = classify_role(rust_path, mirrors);
    if role == "generated build configuration" {
        "Generated or build-probe source; preserve as build evidence, not as a first-class Rust port target.".to_string()
    } else if role == "vendor dependency surface" {
        "Bundled third-party code; prefer dependency, shim, or defer unless behavior requires a port.".to_string()
    } else if role == "test/parity evidence" {
        "Provides parity fixtures and source behavior examples.".to_string()
    } else if role == "core assembly graph model" {
        "Owns graph storage, graph algorithms, coverage, traversal, and path structures."
            .to_string()
    } else if role == "k-mer indexing/counting" {
        "Owns k-mer representation, indexing, counting, and hash-map backed lookup.".to_string()
    } else if role == "sequence/read primitive model" {
        "Owns nucleotide/quality/read sequence primitives used across the pipeline.".to_string()
    } else if role == "read and graph IO" {
        "Owns input/output parsing, read streams, graph serialization, and output formats."
            .to_string()
    } else if role == "read/library metadata" {
        "Owns library/read-set metadata shared by correction and assembly stages.".to_string()
    } else if role == "read correction" {
        "Owns read error-correction stages before assembly graph construction.".to_string()
    } else if role == "paired information/scaffolding" {
        "Owns paired-read distance evidence, path extension, and scaffolding support.".to_string()
    } else if role == "pipeline data/state" || role == "primary assembler pipeline" {
        "Owns stage orchestration for the primary executable pipeline.".to_string()
    } else if role == "graph algorithms and stage modules" || role == "assembly stage orchestration"
    {
        "Owns graph construction, simplification, extension, and assembly-stage algorithms."
            .to_string()
    } else if role == "support tools/output utilities" || role == "visualization/output tooling" {
        "Owns auxiliary command-line tools, converters, reports, and graph/read output support."
            .to_string()
    } else if role == "distributed assembler variant" {
        "Owns MPI/distributed variants of the assembler pipeline.".to_string()
    } else if role == "configuration model" {
        "Owns typed configuration consumed by pipeline stages and project entrypoints.".to_string()
    } else if role == "shared runtime support" {
        "Owns utility types, math, containers, and support code used across core stages."
            .to_string()
    } else {
        "Supporting source cluster inferred from path ownership.".to_string()
    }
}

fn priority_for(role: &str, rust_path: &str) -> u8 {
    if matches!(
        role,
        "configuration model"
            | "pipeline data/state"
            | "sequence/read primitive model"
            | "read and graph IO"
            | "read/library metadata"
            | "k-mer indexing/counting"
            | "core assembly graph model"
            | "graph algorithms and stage modules"
            | "assembly stage orchestration"
            | "primary assembler pipeline"
    ) || rust_path.contains("projects/spades.rs")
    {
        1
    } else if matches!(
        role,
        "read correction"
            | "paired information/scaffolding"
            | "support tools/output utilities"
            | "visualization/output tooling"
            | "shared runtime support"
    ) {
        2
    } else if role.contains("test") {
        4
    } else if role.contains("vendor") || role.contains("generated") {
        5
    } else {
        3
    }
}

fn read_build_targets(path: &Utf8Path) -> Result<Vec<BuildTarget>> {
    let mut seen = BTreeSet::new();
    let mut targets = Vec::new();
    for row in read_jsonl(path)? {
        let target = row
            .get("target")
            .or_else(|| row.get("file"))
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        if target.is_empty() {
            continue;
        }
        let directive = row
            .get("directive")
            .or_else(|| row.get("tool"))
            .and_then(Value::as_str)
            .unwrap_or("build_unit")
            .to_string();
        let build_file = row
            .get("build_file")
            .or_else(|| row.get("directory"))
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        let key = format!("{directive}:{build_file}:{target}");
        if !seen.insert(key) {
            continue;
        }
        let role = classify_build_target(&target, &build_file, &directive);
        targets.push(BuildTarget {
            name: target,
            directive,
            build_file,
            role,
        });
    }
    targets.sort_by(|left, right| {
        (
            build_target_rank(left),
            &left.role,
            &left.build_file,
            &left.name,
        )
            .cmp(&(
                build_target_rank(right),
                &right.role,
                &right.build_file,
                &right.name,
            ))
    });
    Ok(targets)
}

fn classify_build_target(target: &str, build_file: &str, directive: &str) -> String {
    let haystack = format!("{target} {build_file} {directive}").to_lowercase();
    if build_file.starts_with("build_spades/_deps")
        || build_file.starts_with("build_spades/cmakefiles")
        || build_file.contains("/_deps/")
    {
        "generated/external build helper".to_string()
    } else if haystack.contains("ext/") || haystack.contains("vendor") {
        "vendor/library target".to_string()
    } else if haystack.contains("test")
        || haystack.contains("gtest")
        || build_file.contains("src/test/")
    {
        "test target".to_string()
    } else if build_file.contains("src/projects/") && directive == "add_executable" {
        "project executable entrypoint".to_string()
    } else if directive == "add_executable" || haystack.contains("main") {
        "executable or command entrypoint".to_string()
    } else if directive == "add_library" {
        "internal library target".to_string()
    } else if directive == "add_subdirectory" {
        "build subtree".to_string()
    } else {
        "build relationship".to_string()
    }
}

fn build_target_rank(target: &BuildTarget) -> u8 {
    match target.role.as_str() {
        "project executable entrypoint" => 0,
        "executable or command entrypoint" => 1,
        "internal library target" => 2,
        "test target" => 3,
        "build subtree" => 4,
        "build relationship" => 5,
        "vendor/library target" => 6,
        "generated/external build helper" => 7,
        _ => 8,
    }
}

fn read_tool_outcomes(path: &Utf8Path) -> Result<Vec<ToolOutcome>> {
    let outcomes = read_jsonl(path)?
        .into_iter()
        .map(|row| ToolOutcome {
            tool: string_field(&row, "tool"),
            status: string_field(&row, "status"),
            notes: string_field(&row, "notes"),
        })
        .collect::<Vec<_>>();
    Ok(outcomes)
}

fn fact_counts(root: &Utf8Path) -> Result<BTreeMap<String, usize>> {
    let mut counts = BTreeMap::new();
    let facts = root.join("facts");
    if !facts.exists() {
        return Ok(counts);
    }
    for entry in std::fs::read_dir(&facts).with_context(|| format!("read {facts}"))? {
        let entry = entry.with_context(|| format!("read entry in {facts}"))?;
        let path = Utf8PathBuf::from_path_buf(entry.path())
            .map_err(|path| anyhow::anyhow!("non-utf8 path: {}", path.display()))?;
        if path.extension() != Some("jsonl") {
            continue;
        }
        let name = path.file_stem().unwrap_or("unknown").to_string();
        counts.insert(name, count_lines(&path)?);
    }
    Ok(counts)
}

fn read_jsonl(path: &Utf8Path) -> Result<Vec<Value>> {
    if !path.exists() {
        return Ok(Vec::new());
    }
    let file = std::fs::File::open(path).with_context(|| format!("open {path}"))?;
    let reader = std::io::BufReader::new(file);
    let mut rows = Vec::new();
    for line in reader.lines() {
        let line = line.with_context(|| format!("read {path}"))?;
        if line.trim().is_empty() {
            continue;
        }
        if let Ok(row) = serde_json::from_str::<Value>(&line) {
            rows.push(row);
        }
    }
    Ok(rows)
}

fn count_lines(path: &Utf8Path) -> Result<usize> {
    let file = std::fs::File::open(path).with_context(|| format!("open {path}"))?;
    let mut count = 0;
    for line in std::io::BufReader::new(file).lines() {
        line.with_context(|| format!("read {path}"))?;
        count += 1;
    }
    Ok(count)
}

fn string_field(row: &Value, key: &str) -> String {
    row.get(key)
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string()
}

fn joined_or_dash(items: &[String]) -> String {
    if items.is_empty() {
        "-".to_string()
    } else {
        items
            .iter()
            .map(|item| format!("`{}`", item))
            .collect::<Vec<_>>()
            .join(", ")
    }
}

fn markdown_cell(input: &str) -> String {
    input.replace('|', "\\|").replace('\n', " ")
}

fn write_all(path: &Utf8Path, text: &str) -> Result<()> {
    std::fs::write(path, text).with_context(|| format!("write {path}"))
}

fn strip_path_prefix(path: &str, root: &Utf8Path) -> String {
    path.strip_prefix(&format!("{root}/"))
        .unwrap_or(path)
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_core_spades_paths() {
        assert_eq!(
            classify_role("src/common/assembly_graph.rs", &[]),
            "core assembly graph model"
        );
        assert_eq!(
            classify_role("src/projects/hammer.rs", &[]),
            "read correction"
        );
        assert_eq!(
            classify_role("src/vendor/blaze.rs", &[]),
            "vendor dependency surface"
        );
    }
}
