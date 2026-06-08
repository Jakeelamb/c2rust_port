use anyhow::{Context, Result};
use camino::{Utf8Path, Utf8PathBuf};
use chrono::Utc;
use quick_xml::Reader;
use quick_xml::events::{BytesStart, Event};
use rusqlite::{Connection, params};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::io::BufRead;
use std::process::Command;
use walkdir::WalkDir;

use crate::inspect::{ToolStatus, audit_tools};

#[derive(Debug, Serialize)]
struct KnowledgeStrategy {
    goal: String,
    raw_evidence_policy: String,
    consolidation_policy: String,
    repomix_policy: String,
    stages: Vec<KnowledgeStage>,
    fact_tables: Vec<FactTable>,
    dedupe_rules: Vec<DedupeRule>,
    installed_tools: Vec<ToolStatus>,
    missing_tools: Vec<ToolStatus>,
}

#[derive(Debug, Serialize)]
struct KnowledgeStage {
    name: String,
    purpose: String,
    tools: Vec<String>,
    raw_output_dir: String,
    normalized_outputs: Vec<String>,
}

#[derive(Debug, Serialize)]
struct FactTable {
    name: String,
    record_key: String,
    contributors: Vec<String>,
    purpose: String,
}

#[derive(Debug, Serialize)]
struct DedupeRule {
    fact_table: String,
    key: String,
    precedence: Vec<String>,
}

#[derive(Debug, Serialize)]
struct EvidenceRun {
    stage: String,
    tool: String,
    command: Vec<String>,
    status: String,
    exit_code: Option<i32>,
    stdout_path: String,
    stderr_path: String,
    stdout_sha256: String,
    stderr_sha256: String,
    notes: String,
    timestamp: chrono::DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize)]
struct CapabilityRecord {
    name: String,
    category: String,
    purpose: String,
    status: String,
    path: Option<String>,
    evidence_runs: Vec<String>,
    blockers: Vec<String>,
    agent_use: String,
}

#[derive(Debug, Serialize)]
struct SourceRepoMap {
    source_repo: Utf8PathBuf,
    target_repo: Utf8PathBuf,
    files: Vec<RepoFile>,
    process_flow: Vec<ProcessStep>,
    data_flow: Vec<DataFlowEdge>,
    rust_mirror: RustMirrorPlan,
}

#[derive(Debug, Serialize)]
struct RepoFile {
    path: Utf8PathBuf,
    role: String,
    bytes: u64,
    sha256: String,
}

#[derive(Debug, Serialize)]
struct ProcessStep {
    id: String,
    label: String,
    source: String,
    kind: String,
}

#[derive(Debug, Serialize)]
struct DataFlowEdge {
    from: String,
    to: String,
    evidence: String,
}

#[derive(Debug, Serialize)]
struct RustMirrorPlan {
    principle: String,
    modules: Vec<RustModulePlan>,
    entrypoints: Vec<String>,
}

#[derive(Debug, Serialize)]
struct RustModulePlan {
    rust_path: Utf8PathBuf,
    mirrors: Vec<Utf8PathBuf>,
    reason: String,
}

#[derive(Debug, Default, Clone)]
struct DoxygenFacts {
    symbols: Vec<serde_json::Value>,
    types: Vec<serde_json::Value>,
    call_edges: Vec<serde_json::Value>,
}

#[derive(Debug, Default)]
struct DoxygenCompound {
    id: String,
    kind: String,
    name: String,
    file: Option<String>,
    line: Option<u64>,
    brief: String,
    detail: String,
}

#[derive(Debug, Default)]
struct DoxygenMember {
    id: String,
    kind: String,
    name: String,
    type_text: String,
    definition: String,
    args_string: String,
    file: Option<String>,
    line: Option<u64>,
    brief: String,
    detail: String,
    references: Vec<DoxygenReference>,
}

#[derive(Debug, Default)]
struct DoxygenReference {
    refid: Option<String>,
    label: String,
}

#[derive(Debug, Clone)]
struct RustFunction {
    name: String,
    path: Utf8PathBuf,
    line: u64,
    signature: String,
}

const MAX_REPO_MAP_CALL_EDGES: usize = 100_000;
const MAX_NORMALIZED_CFLOW_EDGES: usize = 250_000;

pub fn run(source: &Utf8Path, target: &Utf8Path) -> Result<()> {
    let out = source.join(".c2rust-port/knowledge");
    if out.exists() {
        std::fs::remove_dir_all(&out).with_context(|| format!("remove stale {out}"))?;
    }
    std::fs::create_dir_all(out.join("raw")).with_context(|| format!("create {out}/raw"))?;
    std::fs::create_dir_all(out.join("facts")).with_context(|| format!("create {out}/facts"))?;
    std::fs::create_dir_all(out.join("bundles"))
        .with_context(|| format!("create {out}/bundles"))?;

    let audit = audit_tools();
    let installed_tools = audit
        .iter()
        .filter(|tool| tool.installed)
        .cloned()
        .collect();
    let missing_tools = audit
        .iter()
        .filter(|tool| !tool.installed)
        .cloned()
        .collect();
    let strategy = KnowledgeStrategy {
        goal: "Build the most comprehensive upfront, reusable knowledge base for a C/C++ to Rust port.".to_string(),
        raw_evidence_policy: "Run every installed relevant mapper, tracer, build-capture, benchmark, and Rust analysis tool that can execute safely; preserve raw outputs by tool before normalization.".to_string(),
        consolidation_policy: "Normalize raw outputs into fact tables, dedupe facts by stable keys, preserve provenance for every merged fact, and generate a full-picture bundle for agent packets.".to_string(),
        repomix_policy: "Use repomix when installed as a final bundling layer over selected source files, normalized facts, and summaries; do not use it as the only source of truth.".to_string(),
        stages: stages(),
        fact_tables: fact_tables(),
        dedupe_rules: dedupe_rules(),
        installed_tools,
        missing_tools,
    };

    write_json(&out.join("knowledge-strategy.json"), &strategy)?;
    write_markdown(&out.join("KNOWLEDGE.md"), source, target, &strategy)?;
    write_skeleton_files(&out)?;
    let evidence_runs = collect_raw_evidence(source, target, &out, &strategy)?;
    write_jsonl(&out.join("raw/evidence-runs.jsonl"), &evidence_runs)?;
    write_capability_matrix(&out, &strategy, &evidence_runs)?;
    let repo_map = build_repo_map(source, target)?;
    normalize_facts(source, target, &out, &strategy, &repo_map, &evidence_runs)?;
    write_json(&out.join("repo-map.json"), &repo_map)?;
    write_repo_map_markdown(&out.join("repo-map.md"), &repo_map)?;
    write_mirror_docs(target, &repo_map)?;
    write_evidence_db(&out, &strategy)?;
    write_evidence_queries(&out.join("EVIDENCE_QUERIES.md"))?;
    write_full_picture(
        &out.join("bundles/full-picture.md"),
        &strategy,
        &repo_map,
        &out,
        &evidence_runs,
    )?;
    println!("wrote knowledge strategy to {out}");
    Ok(())
}

fn stages() -> Vec<KnowledgeStage> {
    vec![
        stage(
            "build-capture",
            "Recover authoritative compile commands, include paths, defines, link commands, and build graph shape.",
            &[
                "bear",
                "intercept-build",
                "compiledb",
                "cmake",
                "make",
                "ninja",
                "meson",
                "pkg-config",
            ],
            "raw/build-capture",
            &["facts/build_units.jsonl"],
        ),
        stage(
            "source-structure",
            "Map files, symbols, declarations, definitions, includes, and source-level ownership boundaries.",
            &[
                "clang-query (compile database only)",
                "clangd",
                "ctags",
                "cflow",
                "cscope",
                "doxygen",
            ],
            "raw/source-structure",
            &[
                "facts/files.jsonl",
                "facts/symbols.jsonl",
                "facts/call_edges.jsonl",
            ],
        ),
        stage(
            "semantic-analysis",
            "Extract diagnostics, AST facts, semantic database rows, dataflow hints, and code-property graph facts.",
            &["clang-tidy (compile database only)", "joern", "codeql"],
            "raw/semantic-analysis",
            &["facts/diagnostics.jsonl"],
        ),
        stage(
            "runtime-behavior",
            "Trace file access, dynamic libraries, syscalls, command outputs, timings, profiles, coverage, and debugger evidence.",
            &[
                "strace",
                "ltrace",
                "perf",
                "valgrind",
                "callgrind_annotate",
                "gprof",
                "gcov",
                "llvm-cov",
                "llvm-profdata",
                "lcov",
                "rr",
                "gdb",
                "lldb",
                "bpftrace",
                "hyperfine",
                "time",
                "heaptrack",
            ],
            "raw/runtime-behavior",
            &[
                "facts/runtime_events.jsonl",
                "facts/profiles.jsonl",
                "facts/coverage.jsonl",
            ],
        ),
        stage(
            "benchmark-corpus",
            "Normalize biological benchmark corpora, subset commands, checksums, expected outputs, and parser summaries.",
            &["seqtk", "samtools"],
            "raw/benchmark-corpus",
            &["facts/benchmarks.jsonl"],
        ),
        stage(
            "rust-target",
            "Map the Rust target workspace, APIs, modules, dependencies, tests, coverage, and performance surfaces.",
            &[
                "cargo",
                "rustc",
                "rustdoc",
                "rustfmt",
                "clippy-driver",
                "rust-analyzer",
                "cargo-metadata",
                "cargo-expand",
                "cargo-modules",
                "cargo-udeps",
                "cargo-deny",
                "cargo-nextest",
                "cargo-llvm-cov",
                "cargo-flamegraph",
                "cargo-profiler",
                "cargo-bloat",
                "cargo-asm",
                "cargo-instruments",
            ],
            "raw/rust-target",
            &[
                "facts/rust_workspace.jsonl",
                "facts/diagnostics.jsonl",
                "facts/profiles.jsonl",
                "facts/coverage.jsonl",
            ],
        ),
        stage(
            "bundle",
            "Concatenate selected raw evidence, normalized facts, summaries, source snippets, and packet context into one navigable full-picture bundle.",
            &["repomix"],
            "raw/bundle",
            &["bundles/full-picture.md"],
        ),
    ]
}

fn fact_tables() -> Vec<FactTable> {
    vec![
        fact(
            "tool_runs",
            "tool:timestamp",
            &["all evidence runners"],
            "Every command executed by the mapper, with status, raw output paths, hashes, and notes.",
        ),
        fact(
            "capabilities",
            "tool",
            &["tool-audit", "evidence-runs"],
            "Per-tool readiness matrix: missing, available but unused, ran successfully, or ran with blockers.",
        ),
        fact(
            "files",
            "path",
            &["walkdir", "repomix"],
            "All source, header, generated, test, data, and target files with hashes and roles.",
        ),
        fact(
            "build_units",
            "command_hash",
            &["bear", "cmake", "make", "ninja", "meson"],
            "Compile and link commands, defines, include paths, artifacts, and build targets.",
        ),
        fact(
            "symbols",
            "language:path:name:line",
            &[
                "clang-query",
                "clangd",
                "ctags",
                "doxygen",
                "rust-analyzer",
                "rustdoc",
            ],
            "Functions, types, constants, macros, methods, modules, and public APIs.",
        ),
        fact(
            "types",
            "language:path:name:line",
            &["doxygen", "ctags", "clang-query", "rustdoc"],
            "Classes, structs, unions, enums, typedefs, and their doc comments when available.",
        ),
        fact(
            "call_edges",
            "caller:callee:source_span",
            &[
                "cflow",
                "clang-query",
                "joern",
                "codeql",
                "perf",
                "callgrind_annotate",
            ],
            "Static and dynamic caller-callee evidence with provenance.",
        ),
        fact(
            "dataflow_edges",
            "from:to:evidence",
            &["source-inventory", "repo-map", "cflow", "codeql", "joern"],
            "Include, call, and other source-to-source dependency edges with evidence labels.",
        ),
        fact(
            "feature_tags",
            "entity:feature",
            &["source-inventory", "repo-map"],
            "Heuristic feature labels for narrowing agent queries to indexing, alignment, FM-index, and benchmark areas.",
        ),
        fact(
            "equivalence_edges",
            "cpp_entity:rust_entity",
            &["rust-mirror-plan", "symbols", "rust-source-scan"],
            "Initial source-to-Rust mirror edges plus function-name matches that agents can strengthen after parity evidence.",
        ),
        fact(
            "equivalence_diffs",
            "cpp_entity:rust_entity",
            &["equivalence_edges", "symbols", "rust-source-scan"],
            "Signature and mapping diff rows for cross-repo source-to-Rust matches.",
        ),
        fact(
            "diagnostics",
            "tool:path:line:code",
            &["clang-tidy", "codeql", "cargo", "clippy-driver"],
            "Compile, lint, semantic, and security findings.",
        ),
        fact(
            "semantic_graphs",
            "tool:artifact",
            &["clang-query", "codeql", "joern-parse"],
            "AST, CodeQL database/SARIF, and Joern CPG artifacts that preserve deep semantic structure.",
        ),
        fact(
            "runtime_events",
            "tool:command:event_hash",
            &["strace", "ltrace", "rr", "gdb", "lldb"],
            "Observed runtime behavior, file access, dynamic calls, stack traces, and replay metadata.",
        ),
        fact(
            "profiles",
            "tool:command:function",
            &[
                "perf",
                "valgrind",
                "callgrind_annotate",
                "gprof",
                "cargo-flamegraph",
                "cargo-bloat",
            ],
            "Timing, instruction, allocation, size, and hotspot evidence.",
        ),
        fact(
            "coverage",
            "tool:path:line",
            &["gcov", "llvm-cov", "lcov", "cargo-llvm-cov"],
            "Executed source spans and uncovered behavior.",
        ),
        fact(
            "benchmarks",
            "dataset_id:subset:command",
            &["seqtk", "samtools", "hyperfine", "time"],
            "Dataset subsets, commands, output hashes, timing, memory, stdout, and stderr.",
        ),
        fact(
            "rust_workspace",
            "crate:target:path",
            &[
                "cargo",
                "cargo-metadata",
                "rustdoc",
                "cargo-expand",
                "cargo-modules",
            ],
            "Rust crate graph, modules, target files, feature flags, macro expansions, and docs.",
        ),
        fact(
            "repo_map",
            "node_or_edge_id",
            &["source-inventory", "ctags", "cflow", "clang", "repomix"],
            "Process flow, data flow, and Rust mirror layout for the port.",
        ),
    ]
}

fn dedupe_rules() -> Vec<DedupeRule> {
    vec![
        dedupe("tool_runs", "tool:timestamp", &["evidence-runs"]),
        dedupe("capabilities", "tool", &["evidence-runs", "tool-audit"]),
        dedupe("files", "path", &["source-inventory", "repomix"]),
        dedupe(
            "build_units",
            "command_hash",
            &["compile_commands.json", "bear", "cmake", "make"],
        ),
        dedupe(
            "symbols",
            "language:path:name:line",
            &[
                "clang-query",
                "clangd",
                "ctags",
                "doxygen",
                "rust-analyzer",
                "rustdoc",
            ],
        ),
        dedupe(
            "types",
            "language:path:name:line",
            &["doxygen", "ctags", "clang-query", "rustdoc"],
        ),
        dedupe(
            "call_edges",
            "caller:callee:source_span",
            &[
                "clang-query",
                "joern",
                "codeql",
                "cflow",
                "perf",
                "callgrind_annotate",
            ],
        ),
        dedupe(
            "dataflow_edges",
            "from:to:evidence",
            &["repo-map", "cflow", "codeql", "joern"],
        ),
        dedupe("feature_tags", "entity:feature", &["repo-map"]),
        dedupe(
            "equivalence_edges",
            "cpp_entity:rust_entity",
            &["function-name-match", "rust-mirror-plan"],
        ),
        dedupe(
            "equivalence_diffs",
            "cpp_entity:rust_entity",
            &["function-name-match", "rust-mirror-plan"],
        ),
        dedupe(
            "diagnostics",
            "tool:path:line:code",
            &["compiler", "clang-tidy", "codeql", "clippy"],
        ),
        dedupe(
            "semantic_graphs",
            "tool:artifact",
            &["codeql", "joern-parse", "clang-query"],
        ),
        dedupe(
            "runtime_events",
            "tool:command:event_hash",
            &["rr", "strace", "ltrace", "gdb", "lldb"],
        ),
        dedupe(
            "profiles",
            "tool:command:function",
            &[
                "perf",
                "callgrind_annotate",
                "gprof",
                "cargo-flamegraph",
                "cargo-bloat",
            ],
        ),
        dedupe(
            "coverage",
            "tool:path:line",
            &["llvm-cov", "gcov", "lcov", "cargo-llvm-cov"],
        ),
        dedupe(
            "benchmarks",
            "dataset_id:subset:command",
            &["manifest", "hyperfine", "time"],
        ),
        dedupe(
            "rust_workspace",
            "crate:target:path",
            &["cargo-metadata", "rustdoc", "rust-analyzer", "cargo-expand"],
        ),
        dedupe(
            "repo_map",
            "node_or_edge_id",
            &["repo-map", "cflow", "ctags", "source-inventory"],
        ),
    ]
}

fn stage(
    name: &str,
    purpose: &str,
    tools: &[&str],
    raw_output_dir: &str,
    normalized_outputs: &[&str],
) -> KnowledgeStage {
    KnowledgeStage {
        name: name.to_string(),
        purpose: purpose.to_string(),
        tools: tools.iter().map(|tool| (*tool).to_string()).collect(),
        raw_output_dir: raw_output_dir.to_string(),
        normalized_outputs: normalized_outputs
            .iter()
            .map(|output| (*output).to_string())
            .collect(),
    }
}

fn fact(name: &str, record_key: &str, contributors: &[&str], purpose: &str) -> FactTable {
    FactTable {
        name: name.to_string(),
        record_key: record_key.to_string(),
        contributors: contributors
            .iter()
            .map(|tool| (*tool).to_string())
            .collect(),
        purpose: purpose.to_string(),
    }
}

fn dedupe(fact_table: &str, key: &str, precedence: &[&str]) -> DedupeRule {
    DedupeRule {
        fact_table: fact_table.to_string(),
        key: key.to_string(),
        precedence: precedence.iter().map(|tool| (*tool).to_string()).collect(),
    }
}

fn write_json<T: Serialize>(path: &Utf8Path, value: &T) -> Result<()> {
    std::fs::write(path, serde_json::to_string_pretty(value)?)
        .with_context(|| format!("write {path}"))
}

fn write_markdown(
    path: &Utf8Path,
    source: &Utf8Path,
    target: &Utf8Path,
    strategy: &KnowledgeStrategy,
) -> Result<()> {
    let mut text = String::new();
    text.push_str("# C2Rust Port Knowledge Base\n\n");
    text.push_str(&format!("- Source: `{source}`\n"));
    text.push_str(&format!("- Target: `{target}`\n"));
    text.push_str(&format!("- Goal: {}\n\n", strategy.goal));
    text.push_str("## Policy\n\n");
    text.push_str("- Preserve raw outputs before summarizing.\n");
    text.push_str("- Normalize raw outputs into fact tables.\n");
    text.push_str("- Dedupe by stable keys while retaining provenance.\n");
    text.push_str("- Write `capability-matrix.json` before agent work so missing or blocked tools are explicit.\n");
    text.push_str("- Index normalized rows into `evidence.db` so agents can query facts instead of rediscovering structure.\n");
    text.push_str("- Generate `bundles/full-picture.md` as the agent-consumable map.\n");
    text.push_str("- Use repomix as a final bundling layer when installed, not as the only source of truth.\n\n");
    text.push_str("## Stages\n\n");
    for stage in &strategy.stages {
        text.push_str(&format!(
            "- `{}`: {} Tools: {}. Raw: `{}`. Normalized: `{}`.\n",
            stage.name,
            stage.purpose,
            stage.tools.join(", "),
            stage.raw_output_dir,
            stage.normalized_outputs.join("`, `")
        ));
    }
    text.push_str("\n## Missing Tools\n\n");
    for tool in &strategy.missing_tools {
        text.push_str(&format!(
            "- `{}` ({}) - {}\n",
            tool.name, tool.category, tool.purpose
        ));
    }
    if strategy.missing_tools.is_empty() {
        text.push_str("- None detected.\n");
    }
    std::fs::write(path, text).with_context(|| format!("write {path}"))
}

fn write_skeleton_files(out: &Utf8Path) -> Result<()> {
    for stage in stages() {
        let path = out.join(&stage.raw_output_dir);
        std::fs::create_dir_all(&path).with_context(|| format!("create {path}"))?;
    }

    for name in [
        "tool_runs",
        "capabilities",
        "files",
        "build_units",
        "symbols",
        "types",
        "call_edges",
        "dataflow_edges",
        "feature_tags",
        "equivalence_edges",
        "equivalence_diffs",
        "diagnostics",
        "semantic_graphs",
        "runtime_events",
        "profiles",
        "coverage",
        "benchmarks",
        "rust_workspace",
        "repo_map",
    ] {
        let path = out.join(format!("facts/{name}.jsonl"));
        if !path.exists() {
            std::fs::write(&path, "").with_context(|| format!("write {path}"))?;
        }
    }
    let bundle = out.join("bundles/full-picture.md");
    if !bundle.exists() {
        std::fs::write(
            &bundle,
            "# Full Repo Picture\n\nPending raw evidence collection and normalization.\n",
        )
        .with_context(|| format!("write {bundle}"))?;
    }
    Ok(())
}

fn collect_raw_evidence(
    source: &Utf8Path,
    target: &Utf8Path,
    out: &Utf8Path,
    strategy: &KnowledgeStrategy,
) -> Result<Vec<EvidenceRun>> {
    let installed: BTreeSet<&str> = strategy
        .installed_tools
        .iter()
        .map(|tool| tool.name.as_str())
        .collect();
    let mut runs = Vec::new();
    let source_files = source_files(source)?;

    if installed.contains("make") && source.join("Makefile").exists() {
        runs.push(run_capture(
            source,
            out,
            "build-capture",
            "make",
            &["make", "-n"],
            "dry-run build graph",
        )?);
    }
    if installed.contains("bear") && source.join("Makefile").exists() {
        std::fs::create_dir_all(out.join("raw/build-capture/bear"))
            .with_context(|| format!("create {}", out.join("raw/build-capture/bear")))?;
        let bear_compile_commands = out.join("raw/build-capture/bear/compile_commands.json");
        let args = vec![
            "bear".to_string(),
            "--output".to_string(),
            bear_compile_commands.to_string(),
            "--".to_string(),
            "make".to_string(),
            "-B".to_string(),
        ];
        runs.push(run_capture_owned(
            source,
            out,
            "build-capture",
            "bear",
            &args,
            "full compile database capture through build interception",
        )?);
    }
    if installed.contains("cmake") && source.join("CMakeLists.txt").exists() {
        runs.push(run_capture(
            source,
            out,
            "build-capture",
            "cmake",
            &[
                "cmake",
                "-S",
                ".",
                "-B",
                ".c2rust-port/knowledge/raw/build-capture/cmake-probe",
                "-DCMAKE_EXPORT_COMPILE_COMMANDS=ON",
            ],
            "configure probe for compile database",
        )?);
    }
    let compile_databases = compile_database_paths(source, out)?;
    let compile_db_dir = compile_databases
        .first()
        .and_then(|path| path.parent())
        .map(Utf8Path::to_path_buf);
    let compile_db_units = compile_database_units(&compile_databases)?;
    if installed.contains("clang-tidy") && !compile_db_units.is_empty() {
        let mut args = vec![
            "clang-tidy".to_string(),
            "--quiet".to_string(),
            "-checks=clang-diagnostic-*,bugprone-*,performance-*".to_string(),
        ];
        if let Some(compile_db_dir) = &compile_db_dir {
            args.push("-p".to_string());
            args.push(compile_db_dir.to_string());
        }
        args.extend(
            compile_db_units
                .iter()
                .take(32)
                .map(|file| file.to_string()),
        );
        runs.push(run_capture_owned(
            source,
            out,
            "semantic-analysis",
            "clang-tidy",
            &args,
            "diagnostic checks over bounded source set",
        )?);
    }
    if installed.contains("clang-query") && !compile_db_units.is_empty() {
        let mut args = vec![
            "clang-query".to_string(),
            "-c".to_string(),
            "match functionDecl(isDefinition()).bind(\"function\")".to_string(),
        ];
        if let Some(compile_db_dir) = &compile_db_dir {
            args.push("-p".to_string());
            args.push(compile_db_dir.to_string());
        }
        args.extend(
            compile_db_units
                .iter()
                .take(32)
                .map(|file| file.to_string()),
        );
        runs.push(run_capture_owned(
            source,
            out,
            "source-structure",
            "clang-query",
            &args,
            "AST function declaration extraction",
        )?);
    }
    if installed.contains("ctags") && !source_files.is_empty() {
        let mut args = vec![
            "ctags".to_string(),
            "-x".to_string(),
            "--c-kinds=+p".to_string(),
        ];
        args.extend(source_files.iter().map(|file| file.to_string()));
        runs.push(run_capture_owned(
            source,
            out,
            "source-structure",
            "ctags",
            &args,
            "symbol inventory",
        )?);
    }
    if installed.contains("cflow") && !source_files.is_empty() {
        let mut args = vec!["cflow".to_string()];
        args.extend(source_files.iter().map(|file| file.to_string()));
        runs.push(run_capture_owned(
            source,
            out,
            "source-structure",
            "cflow",
            &args,
            "static call graph",
        )?);
    }
    if installed.contains("cscope") && !source_files.is_empty() {
        let cscope_dir = out.join("raw/source-structure/cscope");
        std::fs::create_dir_all(&cscope_dir).with_context(|| format!("create {cscope_dir}"))?;
        let file_list = cscope_dir.join("cscope.files");
        write_cscope_file_list(&file_list, &source_files)?;
        let database = cscope_dir.join("cscope.out");
        let args = vec![
            "cscope".to_string(),
            "-b".to_string(),
            "-q".to_string(),
            "-k".to_string(),
            "-f".to_string(),
            database.to_string(),
            "-i".to_string(),
            file_list.to_string(),
        ];
        runs.push(run_capture_owned(
            source,
            out,
            "source-structure",
            "cscope",
            &args,
            "symbol and caller/callee cross-reference database",
        )?);
    }
    if installed.contains("doxygen") && !source_files.is_empty() {
        let doxygen_dir = out.join("raw/source-structure/doxygen");
        std::fs::create_dir_all(&doxygen_dir).with_context(|| format!("create {doxygen_dir}"))?;
        let doxyfile = doxygen_dir.join("Doxyfile");
        write_doxygen_config(source, &doxygen_dir, &doxyfile)?;
        let args = vec!["doxygen".to_string(), doxyfile.to_string()];
        runs.push(run_capture_owned(
            source,
            out,
            "source-structure",
            "doxygen",
            &args,
            "Doxygen XML extraction for symbols, types, docs, and reference edges",
        )?);
    }
    if installed.contains("codeql") && source.join("Makefile").exists() {
        std::fs::create_dir_all(out.join("raw/semantic-analysis/codeql"))
            .with_context(|| format!("create {}", out.join("raw/semantic-analysis/codeql")))?;
        let stamp = timestamp_slug();
        let db = out.join(format!("raw/semantic-analysis/codeql/db-{stamp}"));
        let create_args = vec![
            "codeql".to_string(),
            "database".to_string(),
            "create".to_string(),
            db.to_string(),
            "--language=cpp".to_string(),
            "--source-root".to_string(),
            source.to_string(),
            "--command".to_string(),
            "make -B".to_string(),
        ];
        runs.push(run_capture_owned(
            source,
            out,
            "semantic-analysis",
            "codeql-create",
            &create_args,
            "CodeQL C/C++ database creation",
        )?);
        if let Some(query_spec) = discover_codeql_cpp_query_spec() {
            let sarif = out.join(format!(
                "raw/semantic-analysis/codeql/results-{stamp}.sarif"
            ));
            let analyze_args = vec![
                "codeql".to_string(),
                "database".to_string(),
                "analyze".to_string(),
                db.to_string(),
                query_spec,
                "--format=sarif-latest".to_string(),
                "--output".to_string(),
                sarif.to_string(),
            ];
            runs.push(run_capture_owned(
                source,
                out,
                "semantic-analysis",
                "codeql-analyze",
                &analyze_args,
                "CodeQL semantic query export to SARIF",
            )?);
        }
    }
    if installed.contains("joern-parse") {
        std::fs::create_dir_all(out.join("raw/semantic-analysis/joern"))
            .with_context(|| format!("create {}", out.join("raw/semantic-analysis/joern")))?;
        let stamp = timestamp_slug();
        let cpg = out.join(format!("raw/semantic-analysis/joern/cpg-{stamp}.bin"));
        let args = vec![
            "joern-parse".to_string(),
            source.to_string(),
            "-o".to_string(),
            cpg.to_string(),
            "--language".to_string(),
            "C".to_string(),
        ];
        runs.push(run_capture_owned(
            source,
            out,
            "semantic-analysis",
            "joern-parse",
            &args,
            "Joern code property graph creation",
        )?);
    }
    if installed.contains("cargo") && target.join("Cargo.toml").exists() {
        runs.push(run_capture(
            target,
            out,
            "rust-target",
            "cargo-metadata",
            &["cargo", "metadata", "--format-version", "1", "--no-deps"],
            "Rust workspace metadata",
        )?);
        runs.push(run_capture(
            target,
            out,
            "rust-target",
            "cargo-check",
            &["cargo", "check", "--message-format", "short"],
            "Rust compiler diagnostics",
        )?);
    }

    Ok(runs)
}

fn run_capture(
    cwd: &Utf8Path,
    out: &Utf8Path,
    stage: &str,
    tool: &str,
    command: &[&str],
    notes: &str,
) -> Result<EvidenceRun> {
    let owned = command
        .iter()
        .map(|part| (*part).to_string())
        .collect::<Vec<_>>();
    run_capture_owned(cwd, out, stage, tool, &owned, notes)
}

fn run_capture_owned(
    cwd: &Utf8Path,
    out: &Utf8Path,
    stage: &str,
    tool: &str,
    command: &[String],
    notes: &str,
) -> Result<EvidenceRun> {
    let tool_dir = out.join(format!("raw/{stage}/{tool}"));
    std::fs::create_dir_all(&tool_dir).with_context(|| format!("create {tool_dir}"))?;
    let stdout_path = tool_dir.join("stdout.txt");
    let stderr_path = tool_dir.join("stderr.txt");
    let Some((program, args)) = command.split_first() else {
        anyhow::bail!("empty command for {tool}");
    };
    let output = Command::new(program)
        .args(args)
        .current_dir(cwd)
        .output()
        .with_context(|| format!("run {}", command.join(" ")))?;
    std::fs::write(&stdout_path, &output.stdout).with_context(|| format!("write {stdout_path}"))?;
    std::fs::write(&stderr_path, &output.stderr).with_context(|| format!("write {stderr_path}"))?;
    Ok(EvidenceRun {
        stage: stage.to_string(),
        tool: tool.to_string(),
        command: command.to_vec(),
        status: if output.status.success() {
            "ok"
        } else {
            "failed"
        }
        .to_string(),
        exit_code: output.status.code(),
        stdout_path: stdout_path.to_string(),
        stderr_path: stderr_path.to_string(),
        stdout_sha256: sha256_hex(&output.stdout),
        stderr_sha256: sha256_hex(&output.stderr),
        notes: notes.to_string(),
        timestamp: Utc::now(),
    })
}

fn write_capability_matrix(
    out: &Utf8Path,
    strategy: &KnowledgeStrategy,
    evidence_runs: &[EvidenceRun],
) -> Result<()> {
    let records = capability_records(strategy, evidence_runs);
    write_json(&out.join("capability-matrix.json"), &records)?;

    let mut text = String::new();
    text.push_str("# Capability Matrix\n\n");
    text.push_str("This is the source-truth matrix for agent tool use. `ran_ok` means the tool produced raw evidence in this run; `available_unrun` means it is callable but the repo shape did not need it yet; `ran_failed` records the blocker and raw output paths.\n\n");
    text.push_str("| Tool | Category | Status | Agent Use | Evidence / Blocker |\n");
    text.push_str("| --- | --- | --- | --- | --- |\n");
    for record in &records {
        let evidence = if record.blockers.is_empty() {
            record.evidence_runs.join("<br>")
        } else {
            record.blockers.join("<br>")
        };
        text.push_str(&format!(
            "| `{}` | {} | `{}` | {} | {} |\n",
            record.name,
            markdown_table_cell(&record.category),
            record.status,
            markdown_table_cell(&record.agent_use),
            markdown_table_cell(&evidence),
        ));
    }
    std::fs::write(out.join("capability-matrix.md"), text)
        .with_context(|| format!("write {}", out.join("capability-matrix.md")))?;
    Ok(())
}

fn capability_records(
    strategy: &KnowledgeStrategy,
    evidence_runs: &[EvidenceRun],
) -> Vec<CapabilityRecord> {
    strategy
        .installed_tools
        .iter()
        .chain(strategy.missing_tools.iter())
        .map(|tool| {
            let related_runs = evidence_runs
                .iter()
                .filter(|run| tool_matches_run(&tool.name, &run.tool))
                .collect::<Vec<_>>();
            let evidence = related_runs
                .iter()
                .map(|run| {
                    format!(
                        "{}:{}:stdout={}:stderr={}",
                        run.tool, run.status, run.stdout_path, run.stderr_path
                    )
                })
                .collect::<Vec<_>>();
            let blockers = if !tool.installed {
                vec!["not found on PATH".to_string()]
            } else {
                related_runs
                    .iter()
                    .filter(|run| run.status != "ok")
                    .map(|run| {
                        format!(
                            "{} failed{}; inspect {} and {}",
                            run.tool,
                            run.exit_code
                                .map(|code| format!(" with exit {code}"))
                                .unwrap_or_default(),
                            run.stdout_path,
                            run.stderr_path
                        )
                    })
                    .collect()
            };
            let status = if !tool.installed {
                "missing"
            } else if related_runs.iter().any(|run| run.status != "ok") {
                "ran_failed"
            } else if related_runs.iter().any(|run| run.status == "ok") {
                "ran_ok"
            } else {
                "available_unrun"
            };
            CapabilityRecord {
                name: tool.name.clone(),
                category: tool.category.clone(),
                purpose: tool.purpose.clone(),
                status: status.to_string(),
                path: tool.path.clone(),
                evidence_runs: evidence,
                blockers,
                agent_use: agent_use_for_capability(status, &tool.name),
            }
        })
        .collect()
}

fn tool_matches_run(tool_name: &str, run_tool: &str) -> bool {
    tool_name == run_tool
        || (tool_name == "codeql" && run_tool.starts_with("codeql-"))
        || (tool_name == "cargo" && run_tool.starts_with("cargo-"))
}

fn agent_use_for_capability(status: &str, tool_name: &str) -> String {
    match status {
        "ran_ok" => format!("Use `{tool_name}` raw outputs and normalized facts before guessing."),
        "ran_failed" => format!("Read the `{tool_name}` blocker before retrying or relying on it."),
        "available_unrun" => {
            format!("Callable, but no evidence was collected this run; run only for targeted gaps.")
        }
        "missing" => "Do not plan around this tool until installed.".to_string(),
        _ => "Unknown capability state; inspect raw evidence.".to_string(),
    }
}

fn markdown_table_cell(input: &str) -> String {
    input.replace('|', "\\|").replace('\n', "<br>")
}

fn build_repo_map(source: &Utf8Path, target: &Utf8Path) -> Result<SourceRepoMap> {
    let files = repo_files(source)?;
    let function_defs = infer_functions(source)?;
    let process_flow = infer_process_flow(&function_defs);
    let data_flow = infer_data_flow(source, &files, &function_defs)?;
    let rust_mirror = infer_rust_mirror(target, &files, &process_flow);
    Ok(SourceRepoMap {
        source_repo: source.to_path_buf(),
        target_repo: target.to_path_buf(),
        files,
        process_flow,
        data_flow,
        rust_mirror,
    })
}

fn normalize_facts(
    source: &Utf8Path,
    target: &Utf8Path,
    out: &Utf8Path,
    strategy: &KnowledgeStrategy,
    repo_map: &SourceRepoMap,
    evidence_runs: &[EvidenceRun],
) -> Result<()> {
    let doxygen_facts = normalize_doxygen_facts(out)?;
    let symbols = normalize_symbols(out, repo_map, &doxygen_facts)?;
    let types = normalize_types(&doxygen_facts);
    let call_edges = normalize_call_edges(out, repo_map, &doxygen_facts)?;
    let equivalence_edges = normalize_equivalence_edges(target, repo_map, &symbols)?;
    let equivalence_diffs = normalize_equivalence_diffs(&equivalence_edges);

    write_jsonl_values(
        &out.join("facts/tool_runs.jsonl"),
        &normalize_tool_runs(evidence_runs),
    )?;
    write_jsonl_values(
        &out.join("facts/capabilities.jsonl"),
        &normalize_capabilities(strategy, evidence_runs),
    )?;
    write_jsonl_values(
        &out.join("facts/files.jsonl"),
        &normalize_file_facts(&repo_map.files),
    )?;
    write_jsonl_values(
        &out.join("facts/build_units.jsonl"),
        &normalize_build_units(source, out, evidence_runs)?,
    )?;
    write_jsonl_values(&out.join("facts/symbols.jsonl"), &symbols)?;
    write_jsonl_values(&out.join("facts/types.jsonl"), &types)?;
    write_jsonl_values(&out.join("facts/call_edges.jsonl"), &call_edges)?;
    write_jsonl_values(
        &out.join("facts/dataflow_edges.jsonl"),
        &normalize_dataflow_edges(repo_map),
    )?;
    write_jsonl_values(
        &out.join("facts/feature_tags.jsonl"),
        &normalize_feature_tags(repo_map),
    )?;
    write_jsonl_values(
        &out.join("facts/equivalence_edges.jsonl"),
        &equivalence_edges,
    )?;
    write_jsonl_values(
        &out.join("facts/equivalence_diffs.jsonl"),
        &equivalence_diffs,
    )?;
    write_jsonl_values(
        &out.join("facts/diagnostics.jsonl"),
        &normalize_diagnostics(out, evidence_runs)?,
    )?;
    write_jsonl_values(
        &out.join("facts/semantic_graphs.jsonl"),
        &normalize_semantic_graphs(out, evidence_runs)?,
    )?;
    write_jsonl_values(
        &out.join("facts/runtime_events.jsonl"),
        &normalize_runtime_events(out, evidence_runs)?,
    )?;
    write_jsonl_values(
        &out.join("facts/profiles.jsonl"),
        &normalize_profiles(out, evidence_runs)?,
    )?;
    write_jsonl_values(
        &out.join("facts/coverage.jsonl"),
        &normalize_coverage(out, evidence_runs)?,
    )?;
    write_jsonl_values(
        &out.join("facts/benchmarks.jsonl"),
        &normalize_benchmarks(source)?,
    )?;
    write_jsonl_values(
        &out.join("facts/rust_workspace.jsonl"),
        &normalize_rust_workspace(target, out)?,
    )?;
    write_jsonl_values(
        &out.join("facts/repo_map.jsonl"),
        &normalize_repo_map(repo_map),
    )?;
    Ok(())
}

fn normalize_tool_runs(evidence_runs: &[EvidenceRun]) -> Vec<serde_json::Value> {
    evidence_runs
        .iter()
        .map(|run| {
            serde_json::json!({
                "fact_type": "tool_run",
                "key": format!("{}:{}", run.tool, run.timestamp.to_rfc3339()),
                "stage": run.stage,
                "tool": run.tool,
                "command": run.command,
                "status": run.status,
                "exit_code": run.exit_code,
                "stdout_path": run.stdout_path,
                "stderr_path": run.stderr_path,
                "stdout_sha256": run.stdout_sha256,
                "stderr_sha256": run.stderr_sha256,
                "notes": run.notes,
                "timestamp": run.timestamp,
                "provenance": [run.stdout_path, run.stderr_path],
            })
        })
        .collect()
}

fn normalize_capabilities(
    strategy: &KnowledgeStrategy,
    evidence_runs: &[EvidenceRun],
) -> Vec<serde_json::Value> {
    capability_records(strategy, evidence_runs)
        .into_iter()
        .map(|record| {
            serde_json::json!({
                "fact_type": "capability",
                "key": record.name,
                "name": record.name,
                "category": record.category,
                "purpose": record.purpose,
                "status": record.status,
                "path": record.path,
                "evidence_runs": record.evidence_runs,
                "blockers": record.blockers,
                "agent_use": record.agent_use,
                "provenance": ["tool-audit", "raw/evidence-runs.jsonl"],
            })
        })
        .collect()
}

fn normalize_file_facts(files: &[RepoFile]) -> Vec<serde_json::Value> {
    files
        .iter()
        .map(|file| {
            serde_json::json!({
                "fact_type": "file",
                "key": file.path.to_string(),
                "path": file.path,
                "role": file.role,
                "bytes": file.bytes,
                "sha256": file.sha256,
                "provenance": ["repo_walk"],
            })
        })
        .collect()
}

fn normalize_build_units(
    source: &Utf8Path,
    out: &Utf8Path,
    evidence_runs: &[EvidenceRun],
) -> Result<Vec<serde_json::Value>> {
    let mut rows = Vec::new();
    for compile_commands in compile_database_paths(source, out)? {
        rows.extend(parse_compile_commands(&compile_commands)?);
    }
    rows.extend(parse_cmake_build_units(source)?);
    rows.extend(parse_makefile_build_units(source)?);
    for run in evidence_runs.iter().filter(|run| run.tool == "make") {
        for (index, line) in read_lines(&run.stdout_path)?.into_iter().enumerate() {
            if line.trim().is_empty() {
                continue;
            }
            rows.push(serde_json::json!({
                "fact_type": "build_unit",
                "key": sha256_hex(line.as_bytes()),
                "tool": "make",
                "line_index": index,
                "command": line,
                "provenance": [run.stdout_path],
            }));
        }
    }
    let rows = dedupe_rows_by_key_merge_provenance(rows);
    Ok(rows)
}

fn normalize_doxygen_facts(out: &Utf8Path) -> Result<DoxygenFacts> {
    let mut facts = DoxygenFacts::default();
    let xml_dir = out.join("raw/source-structure/doxygen/xml");
    for path in find_paths(xml_dir, Some("xml"))? {
        parse_doxygen_xml_file(&path, &mut facts)?;
    }
    facts.symbols = dedupe_rows_by_key_merge_provenance(facts.symbols);
    facts.types = dedupe_rows_by_key_merge_provenance(facts.types);
    facts.call_edges = dedupe_rows_by_key_merge_provenance(facts.call_edges);
    Ok(facts)
}

fn parse_doxygen_xml_file(path: &Utf8Path, facts: &mut DoxygenFacts) -> Result<()> {
    let xml = std::fs::read_to_string(path).with_context(|| format!("read {path}"))?;
    let mut reader = Reader::from_str(&xml);
    reader.config_mut().trim_text(true);
    let mut stack = Vec::<String>::new();
    let mut compound: Option<DoxygenCompound> = None;
    let mut member: Option<DoxygenMember> = None;
    let mut reference: Option<DoxygenReference> = None;

    loop {
        match reader
            .read_event()
            .with_context(|| format!("parse {path}"))?
        {
            Event::Start(start) => {
                let name = xml_event_name(&start);
                match name.as_str() {
                    "compounddef" => {
                        compound = Some(DoxygenCompound {
                            id: xml_attr(&start, b"id").unwrap_or_default(),
                            kind: xml_attr(&start, b"kind").unwrap_or_default(),
                            ..DoxygenCompound::default()
                        });
                    }
                    "memberdef" => {
                        member = Some(DoxygenMember {
                            id: xml_attr(&start, b"id").unwrap_or_default(),
                            kind: xml_attr(&start, b"kind").unwrap_or_default(),
                            ..DoxygenMember::default()
                        });
                    }
                    "references" => {
                        reference = Some(DoxygenReference {
                            refid: xml_attr(&start, b"refid")
                                .or_else(|| xml_attr(&start, b"compoundref")),
                            label: String::new(),
                        });
                    }
                    "location" => {
                        apply_doxygen_location(&start, &mut compound, &mut member);
                    }
                    _ => {}
                }
                stack.push(name);
            }
            Event::Empty(start) => {
                let name = xml_event_name(&start);
                match name.as_str() {
                    "location" => apply_doxygen_location(&start, &mut compound, &mut member),
                    "references" => {
                        if let Some(member) = member.as_mut() {
                            member.references.push(DoxygenReference {
                                refid: xml_attr(&start, b"refid")
                                    .or_else(|| xml_attr(&start, b"compoundref")),
                                label: xml_attr(&start, b"name").unwrap_or_default(),
                            });
                        }
                    }
                    _ => {}
                }
            }
            Event::Text(text) => {
                if let Ok(text) = text.xml_content() {
                    collect_doxygen_text(
                        text.trim(),
                        &stack,
                        &mut compound,
                        &mut member,
                        &mut reference,
                    );
                }
            }
            Event::CData(text) => {
                if let Ok(text) = text.xml_content() {
                    collect_doxygen_text(
                        text.trim(),
                        &stack,
                        &mut compound,
                        &mut member,
                        &mut reference,
                    );
                }
            }
            Event::End(end) => {
                let name = String::from_utf8_lossy(end.name().as_ref()).to_string();
                match name.as_str() {
                    "references" => {
                        if let (Some(member), Some(reference)) = (member.as_mut(), reference.take())
                        {
                            member.references.push(reference);
                        }
                    }
                    "memberdef" => {
                        if let Some(member) = member.take() {
                            push_doxygen_member_facts(path, member, facts);
                        }
                    }
                    "compounddef" => {
                        if let Some(compound) = compound.take() {
                            push_doxygen_compound_facts(path, compound, facts);
                        }
                    }
                    _ => {}
                }
                let _ = stack.pop();
            }
            Event::Eof => break,
            _ => {}
        }
    }
    Ok(())
}

fn apply_doxygen_location(
    start: &BytesStart<'_>,
    compound: &mut Option<DoxygenCompound>,
    member: &mut Option<DoxygenMember>,
) {
    let file = xml_attr(start, b"file");
    let line = xml_attr(start, b"line").and_then(|line| line.parse::<u64>().ok());
    if let Some(member) = member.as_mut() {
        if file.is_some() {
            member.file = file;
        }
        if line.is_some() {
            member.line = line;
        }
    } else if let Some(compound) = compound.as_mut() {
        if file.is_some() {
            compound.file = file;
        }
        if line.is_some() {
            compound.line = line;
        }
    }
}

fn collect_doxygen_text(
    text: &str,
    stack: &[String],
    compound: &mut Option<DoxygenCompound>,
    member: &mut Option<DoxygenMember>,
    reference: &mut Option<DoxygenReference>,
) {
    if text.is_empty() {
        return;
    }
    if let Some(reference) = reference.as_mut() {
        append_doc_text(&mut reference.label, text);
        return;
    }
    let current = stack.last().map(String::as_str).unwrap_or("");
    if let Some(member) = member.as_mut() {
        match current {
            "name" if is_direct_xml_child(stack, "memberdef", "name") => {
                append_inline_text(&mut member.name, text);
            }
            "type" if is_direct_xml_child(stack, "memberdef", "type") => {
                append_inline_text(&mut member.type_text, text);
            }
            "definition" if is_direct_xml_child(stack, "memberdef", "definition") => {
                append_inline_text(&mut member.definition, text);
            }
            "argsstring" if is_direct_xml_child(stack, "memberdef", "argsstring") => {
                append_inline_text(&mut member.args_string, text);
            }
            _ if stack.iter().any(|tag| tag == "briefdescription") => {
                append_doc_text(&mut member.brief, text);
            }
            _ if stack.iter().any(|tag| tag == "detaileddescription") => {
                append_doc_text(&mut member.detail, text);
            }
            _ => {}
        }
    } else if let Some(compound) = compound.as_mut() {
        match current {
            "compoundname" if is_direct_xml_child(stack, "compounddef", "compoundname") => {
                append_inline_text(&mut compound.name, text);
            }
            _ if stack.iter().any(|tag| tag == "briefdescription") => {
                append_doc_text(&mut compound.brief, text);
            }
            _ if stack.iter().any(|tag| tag == "detaileddescription") => {
                append_doc_text(&mut compound.detail, text);
            }
            _ => {}
        }
    }
}

fn is_direct_xml_child(stack: &[String], parent: &str, child: &str) -> bool {
    stack.last().is_some_and(|tag| tag == child)
        && stack.iter().rev().nth(1).is_some_and(|tag| tag == parent)
}

fn push_doxygen_compound_facts(
    xml_path: &Utf8Path,
    compound: DoxygenCompound,
    facts: &mut DoxygenFacts,
) {
    if !is_doxygen_type_kind(&compound.kind) || compound.name.trim().is_empty() {
        return;
    }
    let key = format!(
        "doxygen:type:{}:{}:{}",
        compound.file.as_deref().unwrap_or("unknown"),
        compound.name,
        compound.line.unwrap_or(0)
    );
    let doc_comment = joined_doc(&compound.brief, &compound.detail);
    facts.symbols.push(serde_json::json!({
        "fact_type": "symbol",
        "key": key,
        "name": compound.name,
        "qualified_name": compound.name,
        "kind": compound.kind,
        "path": compound.file,
        "line": compound.line,
        "signature": null,
        "doc_comment": doc_comment,
        "doxygen_id": compound.id,
        "provenance": [xml_path.to_string()],
    }));
    facts.types.push(serde_json::json!({
        "fact_type": "type",
        "key": format!(
            "doxygen:type:{}:{}:{}",
            compound.file.as_deref().unwrap_or("unknown"),
            compound.name,
            compound.line.unwrap_or(0)
        ),
        "name": compound.name,
        "qualified_name": compound.name,
        "kind": compound.kind,
        "path": compound.file,
        "line": compound.line,
        "doc_comment": doc_comment,
        "doxygen_id": compound.id,
        "fields": [],
        "provenance": [xml_path.to_string()],
    }));
}

fn push_doxygen_member_facts(xml_path: &Utf8Path, member: DoxygenMember, facts: &mut DoxygenFacts) {
    if member.name.trim().is_empty() {
        return;
    }
    let signature = doxygen_member_signature(&member);
    let qualified_name = doxygen_qualified_name(&member);
    let doc_comment = joined_doc(&member.brief, &member.detail);
    facts.symbols.push(serde_json::json!({
        "fact_type": "symbol",
        "key": format!(
            "doxygen:symbol:{}:{}:{}:{}",
            member.file.as_deref().unwrap_or("unknown"),
            member.name,
            member.line.unwrap_or(0),
            member.kind
        ),
        "name": member.name,
        "qualified_name": qualified_name,
        "kind": member.kind,
        "path": member.file,
        "line": member.line,
        "signature": signature,
        "return_type": empty_string_as_null(&member.type_text),
        "doc_comment": doc_comment,
        "doxygen_id": member.id,
        "provenance": [xml_path.to_string()],
    }));

    if is_doxygen_type_kind(&member.kind) {
        facts.types.push(serde_json::json!({
            "fact_type": "type",
            "key": format!(
                "doxygen:type:{}:{}:{}",
                member.file.as_deref().unwrap_or("unknown"),
                member.name,
                member.line.unwrap_or(0)
            ),
            "name": member.name,
            "qualified_name": qualified_name,
            "kind": member.kind,
            "path": member.file,
            "line": member.line,
            "doc_comment": doc_comment,
            "doxygen_id": member.id,
            "fields": [],
            "provenance": [xml_path.to_string()],
        }));
    }

    if is_function_kind(Some(&member.kind)) {
        for reference in member.references {
            let callee = if reference.label.trim().is_empty() {
                reference.refid.unwrap_or_else(|| "unknown".to_string())
            } else {
                reference.label
            };
            facts.call_edges.push(serde_json::json!({
                "fact_type": "call_edge",
                "key": format!("doxygen:{}->{}:{}", qualified_name, callee, member.line.unwrap_or(0)),
                "caller": qualified_name,
                "callee": callee,
                "source_span": signature,
                "evidence": "doxygen reference",
                "provenance": [xml_path.to_string()],
            }));
        }
    }
}

fn xml_event_name(start: &BytesStart<'_>) -> String {
    String::from_utf8_lossy(start.name().as_ref()).to_string()
}

fn xml_attr(start: &BytesStart<'_>, key: &[u8]) -> Option<String> {
    start
        .attributes()
        .with_checks(false)
        .filter_map(Result::ok)
        .find(|attr| attr.key.as_ref() == key)
        .map(|attr| String::from_utf8_lossy(attr.value.as_ref()).to_string())
}

fn append_inline_text(target: &mut String, text: &str) {
    if !target.is_empty() && !target.ends_with(' ') {
        target.push(' ');
    }
    target.push_str(text);
}

fn append_doc_text(target: &mut String, text: &str) {
    if !target.is_empty() && !target.ends_with(' ') {
        target.push(' ');
    }
    target.push_str(text);
}

fn joined_doc(brief: &str, detail: &str) -> Option<String> {
    let joined = [brief.trim(), detail.trim()]
        .into_iter()
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("\n\n");
    (!joined.is_empty()).then_some(joined)
}

fn doxygen_member_signature(member: &DoxygenMember) -> Option<String> {
    let mut signature = if member.definition.trim().is_empty() {
        member.name.clone()
    } else {
        member.definition.clone()
    };
    if !member.args_string.trim().is_empty() {
        signature.push_str(member.args_string.trim());
    }
    (!signature.trim().is_empty()).then_some(signature)
}

fn doxygen_qualified_name(member: &DoxygenMember) -> String {
    if member.definition.trim().is_empty() {
        member.name.clone()
    } else {
        member
            .definition
            .split_whitespace()
            .last()
            .unwrap_or(&member.name)
            .to_string()
    }
}

fn empty_string_as_null(input: &str) -> Option<String> {
    let input = input.trim();
    (!input.is_empty()).then_some(input.to_string())
}

fn is_doxygen_type_kind(kind: &str) -> bool {
    matches!(
        kind,
        "class" | "struct" | "union" | "enum" | "typedef" | "interface"
    )
}

fn normalize_symbols(
    out: &Utf8Path,
    repo_map: &SourceRepoMap,
    doxygen_facts: &DoxygenFacts,
) -> Result<Vec<serde_json::Value>> {
    let mut rows = Vec::new();
    for step in &repo_map.process_flow {
        rows.push(serde_json::json!({
            "fact_type": "symbol",
            "key": format!("source:{}:{}", step.source, step.label),
            "name": step.label,
            "kind": step.kind,
            "path": step.source,
            "line": null,
            "signature": null,
            "provenance": ["repo_map"],
        }));
    }
    let ctags = out.join("raw/source-structure/ctags/stdout.txt");
    if ctags.exists() {
        for line in read_lines(&ctags)? {
            if let Some(symbol) = parse_ctags_line(&line) {
                rows.push(symbol);
            }
        }
    }
    let clang_query = out.join("raw/source-structure/clang-query/stdout.txt");
    if clang_query.exists() {
        rows.extend(parse_clang_query_symbols(&read_lines(&clang_query)?));
    }
    rows.extend(doxygen_facts.symbols.iter().cloned());
    rows.sort_by_key(|row| {
        row.get("key")
            .and_then(|value| value.as_str())
            .unwrap_or("")
            .to_string()
    });
    rows.dedup_by(|left, right| left.get("key") == right.get("key"));
    Ok(rows)
}

fn normalize_types(doxygen_facts: &DoxygenFacts) -> Vec<serde_json::Value> {
    let mut rows = doxygen_facts.types.clone();
    rows.sort_by_key(|row| {
        row.get("key")
            .and_then(|value| value.as_str())
            .unwrap_or("")
            .to_string()
    });
    rows.dedup_by(|left, right| left.get("key") == right.get("key"));
    rows
}

fn normalize_call_edges(
    out: &Utf8Path,
    repo_map: &SourceRepoMap,
    doxygen_facts: &DoxygenFacts,
) -> Result<Vec<serde_json::Value>> {
    let mut rows = Vec::new();
    for edge in repo_map
        .data_flow
        .iter()
        .filter(|edge| edge.to.starts_with("function:"))
    {
        rows.push(serde_json::json!({
            "fact_type": "call_edge",
            "key": format!("{}->{}", edge.from, edge.to),
            "caller": edge.from,
            "callee": edge.to.trim_start_matches("function:"),
            "source_span": null,
            "evidence": edge.evidence,
            "provenance": ["repo_map"],
        }));
    }
    let cflow = out.join("raw/source-structure/cflow/stdout.txt");
    if cflow.exists() {
        rows.extend(parse_cflow_edges_from_path(
            &cflow,
            MAX_NORMALIZED_CFLOW_EDGES,
        )?);
    }
    rows.extend(doxygen_facts.call_edges.iter().cloned());
    rows.sort_by_key(|row| {
        row.get("key")
            .and_then(|value| value.as_str())
            .unwrap_or("")
            .to_string()
    });
    rows.dedup_by(|left, right| left.get("key") == right.get("key"));
    Ok(rows)
}

fn normalize_dataflow_edges(repo_map: &SourceRepoMap) -> Vec<serde_json::Value> {
    repo_map
        .data_flow
        .iter()
        .map(|edge| {
            serde_json::json!({
                "fact_type": "dataflow_edge",
                "key": format!("{}->{}:{}", edge.from, edge.to, edge.evidence),
                "from": edge.from,
                "to": edge.to,
                "evidence": edge.evidence,
                "provenance": ["repo_map"],
            })
        })
        .collect()
}

fn normalize_feature_tags(repo_map: &SourceRepoMap) -> Vec<serde_json::Value> {
    let mut rows = Vec::new();
    for file in &repo_map.files {
        for feature in feature_tags_for_text(file.path.as_str()) {
            rows.push(serde_json::json!({
                "fact_type": "feature_tag",
                "key": format!("file:{}:{feature}", file.path),
                "entity_kind": "file",
                "entity": file.path,
                "feature": feature,
                "evidence": "path keyword heuristic",
                "provenance": ["repo_map"],
            }));
        }
    }
    for step in &repo_map.process_flow {
        let text = format!("{} {}", step.label, step.source);
        for feature in feature_tags_for_text(&text) {
            rows.push(serde_json::json!({
                "fact_type": "feature_tag",
                "key": format!("symbol:{}:{feature}", step.id),
                "entity_kind": "symbol",
                "entity": step.id,
                "feature": feature,
                "evidence": "symbol/path keyword heuristic",
                "provenance": ["repo_map"],
            }));
        }
    }
    rows.sort_by_key(|row| {
        row.get("key")
            .and_then(|value| value.as_str())
            .unwrap_or("")
            .to_string()
    });
    rows.dedup_by(|left, right| left.get("key") == right.get("key"));
    rows
}

fn normalize_equivalence_edges(
    target: &Utf8Path,
    repo_map: &SourceRepoMap,
    symbols: &[serde_json::Value],
) -> Result<Vec<serde_json::Value>> {
    let mut rows = Vec::new();
    for module in &repo_map.rust_mirror.modules {
        for source_path in &module.mirrors {
            rows.push(serde_json::json!({
                "fact_type": "equivalence_edge",
                "key": format!("{}=>{}", source_path, module.rust_path),
                "cpp_entity": source_path,
                "rust_entity": module.rust_path,
                "confidence": "layout",
                "diff_notes": "Initial mirror edge inferred from source ownership cluster; strengthen only after source-backed parity evidence.",
                "provenance": ["rust_mirror_plan"],
            }));
        }
    }
    let rust_functions = infer_rust_functions(target)?;
    let rust_by_name = rust_functions
        .iter()
        .map(|function| (canonical_symbol_name(&function.name), function))
        .collect::<BTreeMap<_, _>>();
    for symbol in symbols.iter().filter(|symbol| {
        is_function_kind(row_string(symbol, "kind").as_deref())
            && row_string(symbol, "path")
                .as_deref()
                .is_some_and(|path| language_for_path(path) != "rust")
    }) {
        let Some(source_name) =
            row_string(symbol, "qualified_name").or_else(|| row_string(symbol, "name"))
        else {
            continue;
        };
        let canonical = canonical_symbol_name(&source_name);
        let Some(rust_function) = rust_by_name.get(&canonical) else {
            continue;
        };
        let source_path = row_string(symbol, "path").unwrap_or_else(|| "unknown".to_string());
        rows.push(serde_json::json!({
            "fact_type": "equivalence_edge",
            "key": format!("function:{}=>{}", source_name, rust_function.path),
            "cpp_entity": source_name,
            "rust_entity": format!("{}:{}", rust_function.path, rust_function.name),
            "confidence": "function-name",
            "match_kind": "canonical_function_name",
            "cpp_path": source_path,
            "cpp_line": row_i64(symbol, "line"),
            "cpp_signature": row_string(symbol, "signature"),
            "rust_path": rust_function.path,
            "rust_line": rust_function.line,
            "rust_signature": rust_function.signature,
            "diff_notes": "C/C++ and Rust functions share a canonical leaf name. This is a comparison target, not behavior parity evidence.",
            "provenance": ["symbols", "rust-source-scan"],
        }));
    }
    rows.sort_by_key(|row| {
        row.get("key")
            .and_then(|value| value.as_str())
            .unwrap_or("")
            .to_string()
    });
    rows.dedup_by(|left, right| left.get("key") == right.get("key"));
    Ok(rows)
}

fn normalize_equivalence_diffs(equivalence_edges: &[serde_json::Value]) -> Vec<serde_json::Value> {
    equivalence_edges
        .iter()
        .filter(|row| row_string(row, "confidence").as_deref() == Some("function-name"))
        .map(|row| {
            let cpp_signature = row_string(row, "cpp_signature");
            let rust_signature = row_string(row, "rust_signature");
            let diff_status = match (cpp_signature.as_deref(), rust_signature.as_deref()) {
                (Some(cpp), Some(rust)) if canonical_signature(cpp) == canonical_signature(rust) => {
                    "signature_text_similar"
                }
                (Some(_), Some(_)) => "signature_text_differs",
                (Some(_), None) => "missing_rust_signature",
                (None, Some(_)) => "missing_cpp_signature",
                (None, None) => "missing_signatures",
            };
            serde_json::json!({
                "fact_type": "equivalence_diff",
                "key": row_string(row, "key").unwrap_or_else(|| "unknown".to_string()),
                "cpp_entity": row_string(row, "cpp_entity"),
                "rust_entity": row_string(row, "rust_entity"),
                "diff_status": diff_status,
                "cpp_signature": cpp_signature,
                "rust_signature": rust_signature,
                "notes": "Function-name equivalence requires source-backed behavior validation before being treated as parity.",
                "provenance": ["equivalence_edges"],
            })
        })
        .collect()
}

fn normalize_diagnostics(
    _out: &Utf8Path,
    evidence_runs: &[EvidenceRun],
) -> Result<Vec<serde_json::Value>> {
    let mut rows = Vec::new();
    for run in evidence_runs.iter().filter(|run| {
        matches!(
            run.tool.as_str(),
            "clang-tidy"
                | "cargo-check"
                | "codeql-create"
                | "codeql-analyze"
                | "doxygen"
                | "joern-parse"
        )
    }) {
        let mut lines = read_lines(&run.stdout_path)?;
        lines.extend(read_lines(&run.stderr_path)?);
        for (index, line) in lines.into_iter().enumerate() {
            if line.trim().is_empty() {
                continue;
            }
            rows.push(serde_json::json!({
                "fact_type": "diagnostic",
                "key": format!("{}:{}:{}", run.tool, index, sha256_hex(line.as_bytes())),
                "tool": run.tool,
                "line_index": index,
                "message": line,
                "status": run.status,
                "provenance": [run.stdout_path, run.stderr_path],
            }));
        }
    }
    rows.extend(normalize_codeql_sarif_diagnostics(_out)?);
    Ok(rows)
}

fn normalize_semantic_graphs(
    out: &Utf8Path,
    evidence_runs: &[EvidenceRun],
) -> Result<Vec<serde_json::Value>> {
    let mut rows = Vec::new();
    for run in evidence_runs.iter().filter(|run| {
        matches!(
            run.tool.as_str(),
            "clang-query" | "codeql-create" | "codeql-analyze" | "doxygen" | "joern-parse"
        )
    }) {
        rows.push(serde_json::json!({
            "fact_type": "semantic_graph_run",
            "key": format!("{}:{}", run.tool, run.timestamp),
            "tool": run.tool,
            "status": run.status,
            "command": run.command,
            "stdout_path": run.stdout_path,
            "stderr_path": run.stderr_path,
            "notes": run.notes,
            "provenance": [run.stdout_path, run.stderr_path],
        }));
    }
    for path in find_paths(out.join("raw/semantic-analysis/codeql"), Some("sarif"))? {
        let bytes = std::fs::read(&path).with_context(|| format!("read {path}"))?;
        rows.push(serde_json::json!({
            "fact_type": "semantic_graph_artifact",
            "key": format!("codeql:sarif:{path}"),
            "tool": "codeql",
            "artifact_kind": "sarif",
            "path": path,
            "bytes": bytes.len(),
            "sha256": sha256_hex(&bytes),
            "provenance": [path.to_string()],
        }));
    }
    for path in find_paths(out.join("raw/semantic-analysis/codeql"), None)?
        .into_iter()
        .filter(|path| path.file_name().is_some_and(|name| name.starts_with("db-")))
    {
        rows.push(serde_json::json!({
            "fact_type": "semantic_graph_artifact",
            "key": format!("codeql:database:{path}"),
            "tool": "codeql",
            "artifact_kind": "database",
            "path": path,
            "provenance": [path.to_string()],
        }));
    }
    for path in find_paths(out.join("raw/semantic-analysis/joern"), Some("bin"))? {
        let bytes = std::fs::read(&path).with_context(|| format!("read {path}"))?;
        rows.push(serde_json::json!({
            "fact_type": "semantic_graph_artifact",
            "key": format!("joern:cpg:{path}"),
            "tool": "joern-parse",
            "artifact_kind": "cpg",
            "path": path,
            "bytes": bytes.len(),
            "sha256": sha256_hex(&bytes),
            "provenance": [path.to_string()],
        }));
    }
    for path in find_paths(out.join("raw/source-structure/doxygen/xml"), Some("xml"))? {
        let bytes = std::fs::read(&path).with_context(|| format!("read {path}"))?;
        rows.push(serde_json::json!({
            "fact_type": "semantic_graph_artifact",
            "key": format!("doxygen:xml:{path}"),
            "tool": "doxygen",
            "artifact_kind": "xml",
            "path": path,
            "bytes": bytes.len(),
            "sha256": sha256_hex(&bytes),
            "provenance": [path.to_string()],
        }));
    }
    Ok(rows)
}

fn normalize_runtime_events(
    _out: &Utf8Path,
    evidence_runs: &[EvidenceRun],
) -> Result<Vec<serde_json::Value>> {
    normalize_stream_lines(
        evidence_runs,
        &["strace", "ltrace", "rr", "gdb", "lldb"],
        "runtime_event",
    )
}

fn normalize_profiles(
    _out: &Utf8Path,
    evidence_runs: &[EvidenceRun],
) -> Result<Vec<serde_json::Value>> {
    normalize_stream_lines(
        evidence_runs,
        &[
            "perf",
            "valgrind",
            "callgrind_annotate",
            "gprof",
            "cargo-flamegraph",
            "cargo-bloat",
        ],
        "profile",
    )
}

fn normalize_coverage(
    _out: &Utf8Path,
    evidence_runs: &[EvidenceRun],
) -> Result<Vec<serde_json::Value>> {
    normalize_stream_lines(
        evidence_runs,
        &["gcov", "llvm-cov", "lcov", "cargo-llvm-cov"],
        "coverage",
    )
}

fn normalize_stream_lines(
    evidence_runs: &[EvidenceRun],
    tools: &[&str],
    fact_type: &str,
) -> Result<Vec<serde_json::Value>> {
    let mut rows = Vec::new();
    for run in evidence_runs
        .iter()
        .filter(|run| tools.contains(&run.tool.as_str()))
    {
        let mut lines = read_lines(&run.stdout_path)?;
        lines.extend(read_lines(&run.stderr_path)?);
        for (index, line) in lines.into_iter().enumerate() {
            if line.trim().is_empty() {
                continue;
            }
            rows.push(serde_json::json!({
                "fact_type": fact_type,
                "key": format!("{}:{}:{}", run.tool, index, sha256_hex(line.as_bytes())),
                "tool": run.tool,
                "line_index": index,
                "line": line,
                "status": run.status,
                "provenance": [run.stdout_path, run.stderr_path],
            }));
        }
    }
    Ok(rows)
}

fn normalize_benchmarks(source: &Utf8Path) -> Result<Vec<serde_json::Value>> {
    let mut rows = Vec::new();
    let manifest_dir = source.join(".c2rust-port/bench/manifests");
    if manifest_dir.is_dir() {
        for entry in
            std::fs::read_dir(&manifest_dir).with_context(|| format!("read {manifest_dir}"))?
        {
            let entry = entry?;
            if !entry.file_type()?.is_file() {
                continue;
            }
            let path = utf8_path(&entry.path())?;
            let text = std::fs::read_to_string(&path).with_context(|| format!("read {path}"))?;
            let value = serde_json::from_str::<serde_json::Value>(&text)
                .with_context(|| format!("parse {path}"))?;
            let key = format!(
                "manifest:{}:{}",
                value
                    .get("dataset_id")
                    .and_then(|value| value.as_str())
                    .unwrap_or("unknown"),
                value
                    .get("subset")
                    .and_then(|value| value.as_str())
                    .unwrap_or("unknown")
            );
            rows.push(serde_json::json!({
                "fact_type": "benchmark_manifest",
                "key": key,
                "manifest": value,
                "provenance": [path.to_string()],
            }));
        }
    }
    let run_dir = source.join(".c2rust-port/bench/runs");
    if run_dir.is_dir() {
        for entry in std::fs::read_dir(&run_dir).with_context(|| format!("read {run_dir}"))? {
            let entry = entry?;
            if !entry.file_type()?.is_file() {
                continue;
            }
            let path = utf8_path(&entry.path())?;
            for (index, line) in read_lines(&path)?.into_iter().enumerate() {
                if line.trim().is_empty() {
                    continue;
                }
                let value = serde_json::from_str::<serde_json::Value>(&line)
                    .unwrap_or_else(|_| serde_json::json!({ "raw": line }));
                rows.push(serde_json::json!({
                    "fact_type": "benchmark_run",
                    "key": format!("run:{}:{index}", path.file_name().unwrap_or("unknown")),
                    "run": value,
                    "provenance": [path.to_string()],
                }));
            }
        }
    }
    rows.sort_by_key(|row| {
        row.get("key")
            .and_then(|value| value.as_str())
            .unwrap_or("")
            .to_string()
    });
    Ok(rows)
}

fn normalize_rust_workspace(target: &Utf8Path, out: &Utf8Path) -> Result<Vec<serde_json::Value>> {
    let mut rows = Vec::new();
    let metadata_path = out.join("raw/rust-target/cargo-metadata/stdout.txt");
    if metadata_path.exists() {
        let text = std::fs::read_to_string(&metadata_path)
            .with_context(|| format!("read {metadata_path}"))?;
        if let Ok(metadata) = serde_json::from_str::<serde_json::Value>(&text) {
            if let Some(packages) = metadata.get("packages").and_then(|value| value.as_array()) {
                for package in packages {
                    let package_name = package
                        .get("name")
                        .and_then(|value| value.as_str())
                        .unwrap_or("unknown");
                    rows.push(serde_json::json!({
                        "fact_type": "rust_package",
                        "key": format!("package:{package_name}"),
                        "package": package_name,
                        "manifest_path": package.get("manifest_path"),
                        "provenance": [metadata_path.to_string()],
                    }));
                    if let Some(targets) = package.get("targets").and_then(|value| value.as_array())
                    {
                        for target_value in targets {
                            let target_name = target_value
                                .get("name")
                                .and_then(|value| value.as_str())
                                .unwrap_or("unknown");
                            rows.push(serde_json::json!({
                                "fact_type": "rust_target",
                                "key": format!("{package_name}:{target_name}"),
                                "package": package_name,
                                "target": target_name,
                                "kind": target_value.get("kind"),
                                "src_path": target_value.get("src_path"),
                                "provenance": [metadata_path.to_string()],
                            }));
                        }
                    }
                }
            }
        }
    } else if target.join("Cargo.toml").exists() {
        rows.push(serde_json::json!({
            "fact_type": "rust_workspace",
            "key": "workspace:unparsed",
            "manifest_path": target.join("Cargo.toml"),
            "provenance": ["Cargo.toml"],
        }));
    }
    Ok(rows)
}

fn normalize_repo_map(repo_map: &SourceRepoMap) -> Vec<serde_json::Value> {
    let mut rows = Vec::new();
    for step in &repo_map.process_flow {
        rows.push(serde_json::json!({
            "fact_type": "repo_map_node",
            "key": step.id,
            "label": step.label,
            "source": step.source,
            "kind": step.kind,
            "provenance": ["repo_map"],
        }));
    }
    for edge in &repo_map.data_flow {
        rows.push(serde_json::json!({
            "fact_type": "repo_map_edge",
            "key": format!("{}->{}:{}", edge.from, edge.to, edge.evidence),
            "from": edge.from,
            "to": edge.to,
            "evidence": edge.evidence,
            "provenance": ["repo_map"],
        }));
    }
    for module in &repo_map.rust_mirror.modules {
        rows.push(serde_json::json!({
            "fact_type": "rust_mirror_module",
            "key": module.rust_path.to_string(),
            "rust_path": module.rust_path,
            "mirrors": module.mirrors,
            "reason": module.reason,
            "provenance": ["repo_map"],
        }));
    }
    rows
}

fn repo_files(source: &Utf8Path) -> Result<Vec<RepoFile>> {
    let mut files = Vec::new();
    for entry in WalkDir::new(source).into_iter().filter_map(Result::ok) {
        if !entry.file_type().is_file() {
            continue;
        }
        let path = utf8_path(entry.path())?;
        if path
            .components()
            .any(|component| component.as_str() == ".c2rust-port")
        {
            continue;
        }
        if is_source_or_build_file(&path) {
            let bytes = std::fs::read(&path).with_context(|| format!("read {path}"))?;
            files.push(RepoFile {
                path: path.strip_prefix(source).unwrap_or(&path).to_path_buf(),
                role: file_role(&path),
                bytes: bytes.len() as u64,
                sha256: sha256_hex(&bytes),
            });
        }
    }
    files.sort_by(|left, right| left.path.cmp(&right.path));
    Ok(files)
}

fn infer_functions(source: &Utf8Path) -> Result<BTreeMap<String, Utf8PathBuf>> {
    let mut functions = BTreeMap::new();
    for file in source_files(source)? {
        let text = std::fs::read_to_string(source.join(&file)).unwrap_or_default();
        for line in text.lines() {
            if let Some(name) = function_name_from_line(line) {
                functions.insert(name, file.clone());
            }
        }
    }
    Ok(functions)
}

fn infer_rust_functions(target: &Utf8Path) -> Result<Vec<RustFunction>> {
    let mut functions = Vec::new();
    if !target.exists() {
        return Ok(functions);
    }
    for entry in WalkDir::new(target).into_iter().filter_map(Result::ok) {
        if !entry.file_type().is_file() {
            continue;
        }
        let path = utf8_path(entry.path())?;
        if path
            .components()
            .any(|component| matches!(component.as_str(), ".git" | ".c-to-rust-port" | "target"))
        {
            continue;
        }
        if path.extension() != Some("rs") {
            continue;
        }
        let relative = path.strip_prefix(target).unwrap_or(&path).to_path_buf();
        for (index, line) in read_lines(&path)?.into_iter().enumerate() {
            let Some(name) = rust_function_name_from_line(&line) else {
                continue;
            };
            functions.push(RustFunction {
                name,
                path: relative.clone(),
                line: (index + 1) as u64,
                signature: line.trim().to_string(),
            });
        }
    }
    functions.sort_by(|left, right| (&left.path, left.line).cmp(&(&right.path, right.line)));
    Ok(functions)
}

fn rust_function_name_from_line(line: &str) -> Option<String> {
    let trimmed = line.trim_start();
    if trimmed.starts_with("//") {
        return None;
    }
    let fn_index = trimmed.find("fn ")?;
    let before = &trimmed[..fn_index];
    if !before.split_whitespace().all(|part| {
        matches!(part, "pub" | "const" | "async" | "unsafe" | "extern") || part.starts_with("pub(")
    }) {
        return None;
    }
    let after = &trimmed[fn_index + 3..];
    let name = after
        .split(|ch: char| !(ch.is_ascii_alphanumeric() || ch == '_'))
        .next()?;
    (!name.is_empty()).then(|| name.to_string())
}

fn infer_process_flow(functions: &BTreeMap<String, Utf8PathBuf>) -> Vec<ProcessStep> {
    let mut steps = Vec::new();
    if let Some(path) = functions.get("main") {
        steps.push(ProcessStep {
            id: "entry:main".to_string(),
            label: "main".to_string(),
            source: path.to_string(),
            kind: "entrypoint".to_string(),
        });
    }
    for (name, path) in functions {
        if name == "main" {
            continue;
        }
        steps.push(ProcessStep {
            id: format!("function:{name}"),
            label: name.clone(),
            source: path.to_string(),
            kind: "function".to_string(),
        });
    }
    steps
}

fn infer_data_flow(
    source: &Utf8Path,
    files: &[RepoFile],
    functions: &BTreeMap<String, Utf8PathBuf>,
) -> Result<Vec<DataFlowEdge>> {
    let mut edges = Vec::new();
    let function_names = functions.keys().cloned().collect::<BTreeSet<_>>();
    let mut call_edges_seen = 0usize;
    for file in files {
        let path = source.join(&file.path);
        let text = std::fs::read_to_string(&path).unwrap_or_default();
        for line in text.lines() {
            if let Some(include) = include_target(line) {
                edges.push(DataFlowEdge {
                    from: file.path.to_string(),
                    to: include,
                    evidence: "include".to_string(),
                });
            }
        }
        if file.role != "source" {
            continue;
        }
        for line in text.lines() {
            for name in function_call_names_in_line(line, &function_names) {
                let Some(target_file) = functions.get(name) else {
                    continue;
                };
                if file.path == *target_file {
                    continue;
                }
                if call_edges_seen >= MAX_REPO_MAP_CALL_EDGES {
                    continue;
                }
                edges.push(DataFlowEdge {
                    from: file.path.to_string(),
                    to: format!("function:{name}"),
                    evidence: "call-site heuristic".to_string(),
                });
                call_edges_seen += 1;
            }
        }
    }
    if call_edges_seen >= MAX_REPO_MAP_CALL_EDGES {
        edges.push(DataFlowEdge {
            from: "repo-map-normalizer".to_string(),
            to: "call-site heuristic edges".to_string(),
            evidence: format!("truncated after {MAX_REPO_MAP_CALL_EDGES} normalized call edges"),
        });
    }
    edges.sort_by(|left, right| {
        (&left.from, &left.to, &left.evidence).cmp(&(&right.from, &right.to, &right.evidence))
    });
    edges.dedup_by(|left, right| {
        left.from == right.from && left.to == right.to && left.evidence == right.evidence
    });
    Ok(edges)
}

fn infer_rust_mirror(
    target: &Utf8Path,
    files: &[RepoFile],
    process_flow: &[ProcessStep],
) -> RustMirrorPlan {
    let mut grouped: BTreeMap<String, Vec<Utf8PathBuf>> = BTreeMap::new();
    for file in files {
        if matches!(file.role.as_str(), "source" | "header") {
            grouped
                .entry(rust_mirror_group(&file.path))
                .or_default()
                .push(file.path.clone());
        }
    }
    let modules = grouped
        .into_iter()
        .map(|(module, mirrors)| RustModulePlan {
            rust_path: target.join(format!("src/{module}.rs")),
            mirrors,
            reason: "Rust module follows source ownership and process boundary cluster".to_string(),
        })
        .collect();
    RustMirrorPlan {
        principle: "Mirror source process boundaries first, then refactor only after behavior parity evidence exists.".to_string(),
        modules,
        entrypoints: process_flow
            .iter()
            .filter(|step| step.kind == "entrypoint")
            .map(|step| step.label.clone())
            .collect(),
    }
}

fn write_repo_map_markdown(path: &Utf8Path, repo_map: &SourceRepoMap) -> Result<()> {
    let mut text = String::new();
    text.push_str("# Source Repo Map\n\n");
    text.push_str("## Process Flow\n\n");
    for step in &repo_map.process_flow {
        text.push_str(&format!(
            "- `{}` ({}) from `{}`\n",
            step.label, step.kind, step.source
        ));
    }
    if repo_map.process_flow.is_empty() {
        text.push_str("- No entrypoints or functions inferred yet.\n");
    }
    text.push_str("\n## Data Flow\n\n");
    for edge in &repo_map.data_flow {
        text.push_str(&format!(
            "- `{}` -> `{}` ({})\n",
            edge.from, edge.to, edge.evidence
        ));
    }
    if repo_map.data_flow.is_empty() {
        text.push_str("- No include or call edges inferred yet.\n");
    }
    text.push_str("\n## Rust Mirror Plan\n\n");
    text.push_str(&format!("{}\n\n", repo_map.rust_mirror.principle));
    for module in &repo_map.rust_mirror.modules {
        text.push_str(&format!(
            "- `{}` mirrors {}\n",
            module.rust_path,
            module
                .mirrors
                .iter()
                .map(|path| format!("`{path}`"))
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    text.push_str("\n## Mermaid Process/Data Flow\n\n");
    text.push_str("```mermaid\nflowchart TD\n");
    for step in &repo_map.process_flow {
        text.push_str(&format!("  {}[\"{}\"]\n", mermaid_id(&step.id), step.label));
    }
    for edge in &repo_map.data_flow {
        text.push_str(&format!(
            "  {} -->|{}| {}\n",
            mermaid_id(&edge.from),
            edge.evidence,
            mermaid_id(&edge.to)
        ));
    }
    text.push_str("```\n");
    std::fs::write(path, text).with_context(|| format!("write {path}"))
}

fn write_mirror_docs(target: &Utf8Path, repo_map: &SourceRepoMap) -> Result<()> {
    let root = target.join(".c-to-rust-port");
    std::fs::create_dir_all(&root).with_context(|| format!("create {root}"))?;
    write_repo_map_markdown(&root.join("SOURCE_REPO_MAP.md"), repo_map)?;
    let mut text = String::new();
    text.push_str("# Rust Mirror Plan\n\n");
    text.push_str(&format!("{}\n\n", repo_map.rust_mirror.principle));
    text.push_str("## Modules\n\n");
    for module in &repo_map.rust_mirror.modules {
        text.push_str(&format!(
            "- `{}` mirrors {}\n",
            module.rust_path,
            module
                .mirrors
                .iter()
                .map(|path| format!("`{path}`"))
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    std::fs::write(root.join("RUST_MIRROR_PLAN.md"), text)
        .with_context(|| format!("write {}", root.join("RUST_MIRROR_PLAN.md")))
}

fn write_full_picture(
    path: &Utf8Path,
    strategy: &KnowledgeStrategy,
    repo_map: &SourceRepoMap,
    out: &Utf8Path,
    evidence_runs: &[EvidenceRun],
) -> Result<()> {
    let mut text = String::new();
    text.push_str("# Full Repo Picture\n\n");
    text.push_str(&format!("- Source: `{}`\n", repo_map.source_repo));
    text.push_str(&format!("- Target: `{}`\n\n", repo_map.target_repo));

    text.push_str("## Executive Summary\n\n");
    text.push_str(&format!("- Goal: {}\n", strategy.goal));
    text.push_str(&format!(
        "- Source inventory: {} mapped files across {} roles.\n",
        repo_map.files.len(),
        role_counts(&repo_map.files).len()
    ));
    text.push_str(&format!(
        "- Build map: {} build units from compile databases, CMake files, Makefiles, and captured build output.\n",
        count_jsonl_rows(&out.join("facts/build_units.jsonl"))?
    ));
    text.push_str(&format!(
        "- Symbol map: {} normalized symbols; process map promotes {} likely functions/entrypoints.\n",
        count_jsonl_rows(&out.join("facts/symbols.jsonl"))?,
        repo_map.process_flow.len()
    ));
    text.push_str(&format!(
        "- Data map: {} normalized call/data edges; {} raw/semantic graph records.\n",
        count_jsonl_rows(&out.join("facts/call_edges.jsonl"))?,
        count_jsonl_rows(&out.join("facts/semantic_graphs.jsonl"))?
    ));
    text.push_str(&format!(
        "- Tool matrix: {} capability records and {} tool-run records indexed for agent queries.\n",
        count_jsonl_rows(&out.join("facts/capabilities.jsonl"))?,
        count_jsonl_rows(&out.join("facts/tool_runs.jsonl"))?
    ));
    text.push_str(&format!(
        "- Rust mirror: {} source ownership clusters.\n\n",
        repo_map.rust_mirror.modules.len()
    ));

    text.push_str("## Build System\n\n");
    text.push_str("- Build-unit evidence is normalized from recursive `compile_commands.json`, nested `CMakeLists.txt`, Makefiles, and captured build output when available.\n");
    text.push_str("- Clang-family AST/lint tools are run only when a compile database is discovered, so missing include paths do not become noisy false failures.\n");
    text.push_str("- Detailed rows: `facts/build_units.jsonl`.\n\n");

    text.push_str("## Tool Outcomes\n\n");
    for run in evidence_runs {
        text.push_str(&format!(
            "- `{}`: {}{} - {}\n",
            run.tool,
            run.status,
            run.exit_code
                .map(|code| format!(" (exit {code})"))
                .unwrap_or_default(),
            run.notes
        ));
    }
    if evidence_runs.is_empty() {
        text.push_str("- No raw evidence tools ran.\n");
    }
    text.push('\n');

    text.push_str("## Entry Points\n\n");
    for entrypoint in &repo_map.rust_mirror.entrypoints {
        text.push_str(&format!("- `{entrypoint}`\n"));
    }
    if repo_map.rust_mirror.entrypoints.is_empty() {
        text.push_str("- No entrypoint inferred from source scan.\n");
    }
    text.push('\n');

    text.push_str("## Source Roles\n\n");
    for (role, count) in role_counts(&repo_map.files) {
        text.push_str(&format!("- `{role}`: {count}\n"));
    }
    text.push('\n');

    text.push_str("## Major Source Subsystems\n\n");
    for module in repo_map.rust_mirror.modules.iter().take(40) {
        text.push_str(&format!(
            "- `{}` mirrors {} source files",
            module.rust_path,
            module.mirrors.len()
        ));
        if let Some(first) = module.mirrors.first() {
            text.push_str(&format!("; starts at `{first}`"));
        }
        text.push('\n');
    }
    if repo_map.rust_mirror.modules.len() > 40 {
        text.push_str(&format!(
            "- ... {} more mirror clusters in `RUST_MIRROR_PLAN.md`.\n",
            repo_map.rust_mirror.modules.len() - 40
        ));
    }
    text.push('\n');

    text.push_str("## Data Flow Evidence\n\n");
    for (evidence, count) in data_flow_counts(&repo_map.data_flow) {
        text.push_str(&format!("- `{evidence}`: {count} edges\n"));
    }
    text.push_str("- Detailed source map: `repo-map.md` and target-side `.c-to-rust-port/SOURCE_REPO_MAP.md`.\n\n");

    text.push_str("## Rust Mirror Plan\n\n");
    text.push_str(&format!("{}\n\n", repo_map.rust_mirror.principle));
    text.push_str("- Port modules should initially follow the source ownership clusters above.\n");
    text.push_str("- Do not refactor across clusters until behavior parity evidence exists.\n");
    text.push_str("- Detailed mirror plan: target-side `.c-to-rust-port/RUST_MIRROR_PLAN.md`.\n\n");

    text.push_str("## Fact Tables\n\n");
    text.push_str("All normalized rows are also indexed in `evidence.db` for SQLite queries. Use JSONL for raw citation text and SQLite for fast filtering.\n\n");
    text.push_str("- Query recipes: `EVIDENCE_QUERIES.md`.\n");
    for table in &strategy.fact_tables {
        text.push_str(&format!(
            "- `facts/{}.jsonl`: {} rows. {}\n",
            table.name,
            count_jsonl_rows(&out.join(format!("facts/{}.jsonl", table.name)))?,
            table.purpose
        ));
    }
    text.push_str("\n## Raw Evidence\n\n");
    text.push_str("- SQLite evidence index: `evidence.db`.\n");
    text.push_str("- SQLite query recipes: `EVIDENCE_QUERIES.md`.\n");
    text.push_str(
        "- Tool capability matrix: `capability-matrix.json` and `capability-matrix.md`.\n",
    );
    text.push_str("- Tool run ledger: `raw/evidence-runs.jsonl`.\n");
    text.push_str("- Source structure: `raw/source-structure/`.\n");
    text.push_str("- Semantic analysis: `raw/semantic-analysis/`.\n");
    text.push_str("- Build capture: `raw/build-capture/`.\n");
    text.push_str("- Rust target: `raw/rust-target/`.\n");
    std::fs::write(path, text).with_context(|| format!("write {path}"))
}

fn write_evidence_db(out: &Utf8Path, strategy: &KnowledgeStrategy) -> Result<()> {
    let db_path = out.join("evidence.db");
    if db_path.exists() {
        std::fs::remove_file(&db_path).with_context(|| format!("remove stale {db_path}"))?;
    }
    let mut conn =
        Connection::open(db_path.as_std_path()).with_context(|| format!("open {db_path}"))?;
    conn.execute_batch(
        "
        PRAGMA journal_mode = WAL;
        PRAGMA synchronous = OFF;
        CREATE TABLE facts (
            table_name TEXT NOT NULL,
            key TEXT NOT NULL,
            fact_type TEXT,
            json TEXT NOT NULL,
            provenance_json TEXT NOT NULL,
            PRIMARY KEY (table_name, key)
        );
        CREATE TABLE tool_runs (
            key TEXT PRIMARY KEY,
            stage TEXT,
            tool TEXT,
            command_json TEXT,
            status TEXT,
            exit_code INTEGER,
            stdout_path TEXT,
            stderr_path TEXT,
            notes TEXT,
            timestamp TEXT,
            json TEXT NOT NULL
        );
        CREATE TABLE capabilities (
            name TEXT PRIMARY KEY,
            category TEXT,
            purpose TEXT,
            status TEXT,
            path TEXT,
            evidence_runs_json TEXT,
            blockers_json TEXT,
            agent_use TEXT,
            json TEXT NOT NULL
        );
        CREATE TABLE files (
            path TEXT PRIMARY KEY,
            role TEXT,
            bytes INTEGER,
            sha256 TEXT,
            json TEXT NOT NULL
        );
        CREATE TABLE symbols (
            key TEXT PRIMARY KEY,
            name TEXT,
            kind TEXT,
            path TEXT,
            line INTEGER,
            signature TEXT,
            provenance_json TEXT NOT NULL,
            json TEXT NOT NULL
        );
        CREATE TABLE functions (
            key TEXT PRIMARY KEY,
            language TEXT,
            qualified_name TEXT,
            signature TEXT,
            return_type TEXT,
            file TEXT,
            line INTEGER,
            doc_comment TEXT,
            json TEXT NOT NULL
        );
        CREATE TABLE types (
            key TEXT PRIMARY KEY,
            language TEXT,
            qualified_name TEXT,
            kind TEXT,
            file TEXT,
            line INTEGER,
            fields_json TEXT,
            json TEXT NOT NULL
        );
        CREATE TABLE call_edges (
            key TEXT PRIMARY KEY,
            caller TEXT,
            callee TEXT,
            source_span TEXT,
            evidence TEXT,
            provenance_json TEXT NOT NULL,
            json TEXT NOT NULL
        );
        CREATE TABLE dataflow_edges (
            key TEXT PRIMARY KEY,
            source TEXT,
            target TEXT,
            evidence TEXT,
            provenance_json TEXT NOT NULL,
            json TEXT NOT NULL
        );
        CREATE TABLE feature_tags (
            key TEXT PRIMARY KEY,
            entity_kind TEXT,
            entity TEXT,
            feature TEXT,
            evidence TEXT,
            provenance_json TEXT NOT NULL,
            json TEXT NOT NULL
        );
        CREATE TABLE equivalence_edges (
            key TEXT PRIMARY KEY,
            cpp_entity TEXT,
            rust_entity TEXT,
            confidence TEXT,
            diff_notes TEXT,
            provenance_json TEXT NOT NULL,
            json TEXT NOT NULL
        );
        CREATE TABLE equivalence_diffs (
            key TEXT PRIMARY KEY,
            cpp_entity TEXT,
            rust_entity TEXT,
            diff_status TEXT,
            cpp_signature TEXT,
            rust_signature TEXT,
            notes TEXT,
            provenance_json TEXT NOT NULL,
            json TEXT NOT NULL
        );
        CREATE INDEX idx_facts_table ON facts(table_name);
        CREATE INDEX idx_symbols_name ON symbols(name);
        CREATE INDEX idx_symbols_path ON symbols(path);
        CREATE INDEX idx_types_name ON types(qualified_name);
        CREATE INDEX idx_functions_name ON functions(qualified_name);
        CREATE INDEX idx_call_edges_caller ON call_edges(caller);
        CREATE INDEX idx_call_edges_callee ON call_edges(callee);
        CREATE INDEX idx_dataflow_source ON dataflow_edges(source);
        CREATE INDEX idx_dataflow_target ON dataflow_edges(target);
        CREATE INDEX idx_feature_tags_feature ON feature_tags(feature);
        ",
    )
    .with_context(|| format!("create schema in {db_path}"))?;

    let tx = conn.transaction()?;
    for table in &strategy.fact_tables {
        let path = out.join(format!("facts/{}.jsonl", table.name));
        for row in read_jsonl_values(&path)? {
            insert_evidence_row(&tx, &table.name, &row)?;
        }
    }
    tx.commit()
        .with_context(|| format!("commit evidence db {db_path}"))?;
    Ok(())
}

fn write_evidence_queries(path: &Utf8Path) -> Result<()> {
    let mut text = String::new();
    text.push_str("# Evidence DB Query Recipes\n\n");
    text.push_str("Run from the source repo after `c2rust-port <repo>`:\n\n");
    text.push_str("```bash\nsqlite3 .c2rust-port/knowledge/evidence.db\n```\n\n");
    text.push_str("## Find Indexing Functions\n\n");
    text.push_str("```sql\n");
    text.push_str("SELECT f.qualified_name, f.file, f.line, f.signature\n");
    text.push_str("FROM functions f\n");
    text.push_str("LEFT JOIN feature_tags t ON t.entity = f.key OR t.entity = f.qualified_name OR t.entity = f.file\n");
    text.push_str("WHERE t.feature = 'indexing'\n");
    text.push_str("   OR lower(f.qualified_name) LIKE '%index%'\n");
    text.push_str("   OR lower(f.qualified_name) LIKE '%bwt%'\n");
    text.push_str("   OR lower(f.qualified_name) LIKE '%suffix%'\n");
    text.push_str("ORDER BY f.file, f.line\nLIMIT 100;\n");
    text.push_str("```\n\n");
    text.push_str("## Show Callees For A Function\n\n");
    text.push_str("```sql\n");
    text.push_str("SELECT caller, callee, evidence, source_span\n");
    text.push_str(
        "FROM call_edges\nWHERE caller LIKE '%FUNCTION_OR_FILE%'\nORDER BY callee\nLIMIT 200;\n",
    );
    text.push_str("```\n\n");
    text.push_str("## Show Data Flow Around Indexing Files\n\n");
    text.push_str("```sql\n");
    text.push_str("SELECT source, target, evidence\n");
    text.push_str("FROM dataflow_edges\n");
    text.push_str("WHERE lower(source) LIKE '%idx%' OR lower(target) LIKE '%idx%'\n");
    text.push_str("   OR lower(source) LIKE '%bwt%' OR lower(target) LIKE '%bwt%'\n");
    text.push_str("ORDER BY source, target\nLIMIT 200;\n");
    text.push_str("```\n\n");
    text.push_str("## List Failed Or Blocked Tools\n\n");
    text.push_str("```sql\n");
    text.push_str("SELECT name, status, blockers_json, agent_use\n");
    text.push_str("FROM capabilities\nWHERE status IN ('ran_failed', 'missing')\nORDER BY name;\n");
    text.push_str("```\n\n");
    text.push_str("## Compare Source And Rust Mirror Candidates\n\n");
    text.push_str("```sql\n");
    text.push_str("SELECT cpp_entity, rust_entity, confidence, diff_notes\n");
    text.push_str("FROM equivalence_edges\nORDER BY confidence DESC, cpp_entity\nLIMIT 200;\n");
    text.push_str("```\n\n");
    text.push_str("## Show Function-Level Diff Rows\n\n");
    text.push_str("```sql\n");
    text.push_str(
        "SELECT cpp_entity, rust_entity, diff_status, cpp_signature, rust_signature, notes\n",
    );
    text.push_str("FROM equivalence_diffs\nORDER BY diff_status, cpp_entity\nLIMIT 200;\n");
    text.push_str("```\n\n");
    text.push_str("## Pull Raw JSON For A Citation\n\n");
    text.push_str("```sql\n");
    text.push_str(
        "SELECT json FROM facts WHERE table_name = 'symbols' AND key = 'PASTE_FACT_KEY';\n",
    );
    text.push_str("```\n");
    std::fs::write(path, text).with_context(|| format!("write {path}"))
}

fn insert_evidence_row(
    tx: &rusqlite::Transaction<'_>,
    table_name: &str,
    row: &serde_json::Value,
) -> Result<()> {
    let json = serde_json::to_string(row)?;
    let key = row_string(row, "key").unwrap_or_else(|| sha256_hex(json.as_bytes()));
    let fact_type = row_string(row, "fact_type");
    let provenance = row_json_text(row, "provenance")?;
    tx.execute(
        "INSERT OR REPLACE INTO facts(table_name, key, fact_type, json, provenance_json)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params![table_name, key, fact_type, json, provenance],
    )?;

    match table_name {
        "tool_runs" => insert_tool_run_row(tx, row),
        "capabilities" => insert_capability_row(tx, row),
        "files" => insert_file_row(tx, row),
        "symbols" => insert_symbol_row(tx, row),
        "types" => insert_type_row(tx, row),
        "call_edges" => insert_call_edge_row(tx, row),
        "dataflow_edges" => insert_dataflow_edge_row(tx, row),
        "feature_tags" => insert_feature_tag_row(tx, row),
        "equivalence_edges" => insert_equivalence_edge_row(tx, row),
        "equivalence_diffs" => insert_equivalence_diff_row(tx, row),
        _ => Ok(()),
    }
}

fn insert_tool_run_row(tx: &rusqlite::Transaction<'_>, row: &serde_json::Value) -> Result<()> {
    let json = serde_json::to_string(row)?;
    tx.execute(
        "INSERT OR REPLACE INTO tool_runs(
            key, stage, tool, command_json, status, exit_code, stdout_path, stderr_path, notes,
            timestamp, json
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
        params![
            row_string(row, "key"),
            row_string(row, "stage"),
            row_string(row, "tool"),
            row_json_text(row, "command")?,
            row_string(row, "status"),
            row_i64(row, "exit_code"),
            row_string(row, "stdout_path"),
            row_string(row, "stderr_path"),
            row_string(row, "notes"),
            row_string(row, "timestamp"),
            json,
        ],
    )?;
    Ok(())
}

fn insert_capability_row(tx: &rusqlite::Transaction<'_>, row: &serde_json::Value) -> Result<()> {
    let json = serde_json::to_string(row)?;
    tx.execute(
        "INSERT OR REPLACE INTO capabilities(
            name, category, purpose, status, path, evidence_runs_json, blockers_json, agent_use, json
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        params![
            row_string(row, "name"),
            row_string(row, "category"),
            row_string(row, "purpose"),
            row_string(row, "status"),
            row_string(row, "path"),
            row_json_text(row, "evidence_runs")?,
            row_json_text(row, "blockers")?,
            row_string(row, "agent_use"),
            json,
        ],
    )?;
    Ok(())
}

fn insert_file_row(tx: &rusqlite::Transaction<'_>, row: &serde_json::Value) -> Result<()> {
    let json = serde_json::to_string(row)?;
    tx.execute(
        "INSERT OR REPLACE INTO files(path, role, bytes, sha256, json)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params![
            row_string(row, "path"),
            row_string(row, "role"),
            row_i64(row, "bytes"),
            row_string(row, "sha256"),
            json,
        ],
    )?;
    Ok(())
}

fn insert_symbol_row(tx: &rusqlite::Transaction<'_>, row: &serde_json::Value) -> Result<()> {
    let json = serde_json::to_string(row)?;
    let key = row_string(row, "key");
    let name = row_string(row, "name");
    let kind = row_string(row, "kind");
    let path = row_string(row, "path");
    let line = row_i64(row, "line");
    let signature = row_string(row, "signature");
    let provenance = row_json_text(row, "provenance")?;
    tx.execute(
        "INSERT OR REPLACE INTO symbols(
            key, name, kind, path, line, signature, provenance_json, json
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![key, name, kind, path, line, signature, provenance, json,],
    )?;

    if is_function_kind(kind.as_deref()) {
        tx.execute(
            "INSERT OR REPLACE INTO functions(
                key, language, qualified_name, signature, return_type, file, line, doc_comment, json
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                row_string(row, "key"),
                path.as_deref().map(language_for_path),
                row_string(row, "qualified_name").or(name.clone()),
                signature,
                row_string(row, "return_type"),
                path,
                line,
                row_string(row, "doc_comment"),
                json,
            ],
        )?;
    } else if is_type_kind(kind.as_deref()) {
        tx.execute(
            "INSERT OR REPLACE INTO types(
                key, language, qualified_name, kind, file, line, fields_json, json
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                row_string(row, "key"),
                path.as_deref().map(language_for_path),
                name,
                kind,
                path,
                line,
                row_json_text(row, "fields")?,
                json,
            ],
        )?;
    }
    Ok(())
}

fn insert_type_row(tx: &rusqlite::Transaction<'_>, row: &serde_json::Value) -> Result<()> {
    let json = serde_json::to_string(row)?;
    let path = row_string(row, "path");
    tx.execute(
        "INSERT OR REPLACE INTO types(
            key, language, qualified_name, kind, file, line, fields_json, json
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![
            row_string(row, "key"),
            path.as_deref().map(language_for_path),
            row_string(row, "qualified_name").or_else(|| row_string(row, "name")),
            row_string(row, "kind"),
            path,
            row_i64(row, "line"),
            row_json_text(row, "fields")?,
            json,
        ],
    )?;
    Ok(())
}

fn insert_call_edge_row(tx: &rusqlite::Transaction<'_>, row: &serde_json::Value) -> Result<()> {
    let json = serde_json::to_string(row)?;
    tx.execute(
        "INSERT OR REPLACE INTO call_edges(
            key, caller, callee, source_span, evidence, provenance_json, json
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![
            row_string(row, "key"),
            row_string(row, "caller"),
            row_string(row, "callee"),
            row_string(row, "source_span"),
            row_string(row, "evidence"),
            row_json_text(row, "provenance")?,
            json,
        ],
    )?;
    Ok(())
}

fn insert_dataflow_edge_row(tx: &rusqlite::Transaction<'_>, row: &serde_json::Value) -> Result<()> {
    let json = serde_json::to_string(row)?;
    tx.execute(
        "INSERT OR REPLACE INTO dataflow_edges(
            key, source, target, evidence, provenance_json, json
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![
            row_string(row, "key"),
            row_string(row, "from"),
            row_string(row, "to"),
            row_string(row, "evidence"),
            row_json_text(row, "provenance")?,
            json,
        ],
    )?;
    Ok(())
}

fn insert_feature_tag_row(tx: &rusqlite::Transaction<'_>, row: &serde_json::Value) -> Result<()> {
    let json = serde_json::to_string(row)?;
    tx.execute(
        "INSERT OR REPLACE INTO feature_tags(
            key, entity_kind, entity, feature, evidence, provenance_json, json
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![
            row_string(row, "key"),
            row_string(row, "entity_kind"),
            row_string(row, "entity"),
            row_string(row, "feature"),
            row_string(row, "evidence"),
            row_json_text(row, "provenance")?,
            json,
        ],
    )?;
    Ok(())
}

fn insert_equivalence_edge_row(
    tx: &rusqlite::Transaction<'_>,
    row: &serde_json::Value,
) -> Result<()> {
    let json = serde_json::to_string(row)?;
    tx.execute(
        "INSERT OR REPLACE INTO equivalence_edges(
            key, cpp_entity, rust_entity, confidence, diff_notes, provenance_json, json
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![
            row_string(row, "key"),
            row_string(row, "cpp_entity"),
            row_string(row, "rust_entity"),
            row_string(row, "confidence"),
            row_string(row, "diff_notes"),
            row_json_text(row, "provenance")?,
            json,
        ],
    )?;
    Ok(())
}

fn insert_equivalence_diff_row(
    tx: &rusqlite::Transaction<'_>,
    row: &serde_json::Value,
) -> Result<()> {
    let json = serde_json::to_string(row)?;
    tx.execute(
        "INSERT OR REPLACE INTO equivalence_diffs(
            key, cpp_entity, rust_entity, diff_status, cpp_signature, rust_signature, notes,
            provenance_json, json
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        params![
            row_string(row, "key"),
            row_string(row, "cpp_entity"),
            row_string(row, "rust_entity"),
            row_string(row, "diff_status"),
            row_string(row, "cpp_signature"),
            row_string(row, "rust_signature"),
            row_string(row, "notes"),
            row_json_text(row, "provenance")?,
            json,
        ],
    )?;
    Ok(())
}

fn read_jsonl_values(path: &Utf8Path) -> Result<Vec<serde_json::Value>> {
    let mut rows = Vec::new();
    for (line_index, line) in read_lines(path)?.into_iter().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        rows.push(
            serde_json::from_str(&line)
                .with_context(|| format!("parse {path} line {}", line_index + 1))?,
        );
    }
    Ok(rows)
}

fn row_string(row: &serde_json::Value, key: &str) -> Option<String> {
    row.get(key).and_then(|value| match value {
        serde_json::Value::String(text) => Some(text.clone()),
        serde_json::Value::Number(number) => Some(number.to_string()),
        serde_json::Value::Bool(value) => Some(value.to_string()),
        _ => None,
    })
}

fn row_i64(row: &serde_json::Value, key: &str) -> Option<i64> {
    row.get(key).and_then(|value| value.as_i64())
}

fn row_json_text(row: &serde_json::Value, key: &str) -> Result<String> {
    serde_json::to_string(row.get(key).unwrap_or(&serde_json::Value::Null))
        .with_context(|| format!("serialize json field {key}"))
}

fn is_function_kind(kind: Option<&str>) -> bool {
    kind.is_some_and(|kind| {
        let lower = kind.to_ascii_lowercase();
        matches!(
            lower.as_str(),
            "function" | "entrypoint" | "method" | "prototype"
        ) || lower.contains("function")
    })
}

fn is_type_kind(kind: Option<&str>) -> bool {
    kind.is_some_and(|kind| {
        matches!(
            kind.to_ascii_lowercase().as_str(),
            "class" | "struct" | "union" | "enum" | "typedef" | "interface"
        )
    })
}

fn language_for_path(path: &str) -> &'static str {
    match Utf8Path::new(path).extension() {
        Some("rs") => "rust",
        Some("c" | "h") => "c",
        Some("cc" | "cpp" | "cxx" | "C" | "hh" | "hpp" | "hxx") => "cpp",
        _ => "unknown",
    }
}

fn count_jsonl_rows(path: &Utf8Path) -> Result<usize> {
    if !path.exists() {
        return Ok(0);
    }
    let file = std::fs::File::open(path).with_context(|| format!("open {path}"))?;
    Ok(std::io::BufReader::new(file).lines().count())
}

fn role_counts(files: &[RepoFile]) -> BTreeMap<String, usize> {
    let mut counts = BTreeMap::new();
    for file in files {
        *counts.entry(file.role.clone()).or_insert(0) += 1;
    }
    counts
}

fn data_flow_counts(edges: &[DataFlowEdge]) -> BTreeMap<String, usize> {
    let mut counts = BTreeMap::new();
    for edge in edges {
        *counts.entry(edge.evidence.clone()).or_insert(0) += 1;
    }
    counts
}

fn feature_tags_for_text(text: &str) -> Vec<&'static str> {
    let lower = text.to_ascii_lowercase();
    let mut tags = Vec::new();
    if [
        "index",
        "idx",
        "bwt",
        "ebwt",
        "fm",
        "suffix",
        "sais",
        "blockwise",
        "build",
    ]
    .iter()
    .any(|needle| lower.contains(needle))
    {
        tags.push("indexing");
    }
    if ["align", "seed", "score", "extend", "sam", "read", "pair"]
        .iter()
        .any(|needle| lower.contains(needle))
    {
        tags.push("alignment");
    }
    if ["bwt", "ebwt", "fm_index", "fm-index", "fmindex"]
        .iter()
        .any(|needle| lower.contains(needle))
    {
        tags.push("fm_index");
    }
    if ["test", "bench", "example", "fixture", "lambda"]
        .iter()
        .any(|needle| lower.contains(needle))
    {
        tags.push("validation");
    }
    tags.sort();
    tags.dedup();
    tags
}

fn canonical_symbol_name(name: &str) -> String {
    name.rsplit("::")
        .next()
        .unwrap_or(name)
        .trim_start_matches('~')
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric() || *ch == '_')
        .flat_map(char::to_lowercase)
        .collect()
}

fn canonical_signature(signature: &str) -> String {
    signature
        .chars()
        .filter(|ch| !ch.is_whitespace())
        .flat_map(char::to_lowercase)
        .collect()
}

fn source_files(source: &Utf8Path) -> Result<Vec<Utf8PathBuf>> {
    let mut files = Vec::new();
    for entry in WalkDir::new(source).into_iter().filter_map(Result::ok) {
        if !entry.file_type().is_file() {
            continue;
        }
        let path = utf8_path(entry.path())?;
        if path
            .components()
            .any(|component| component.as_str() == ".c2rust-port")
        {
            continue;
        }
        if matches!(
            path.extension(),
            Some("c" | "cc" | "cpp" | "cxx" | "C" | "h" | "hh" | "hpp" | "hxx")
        ) {
            files.push(path.strip_prefix(source).unwrap_or(&path).to_path_buf());
        }
    }
    files.sort();
    Ok(files)
}

fn write_cscope_file_list(path: &Utf8Path, source_files: &[Utf8PathBuf]) -> Result<()> {
    let mut text = String::new();
    for source_file in source_files {
        text.push_str(source_file.as_str());
        text.push('\n');
    }
    std::fs::write(path, text).with_context(|| format!("write {path}"))
}

fn write_doxygen_config(source: &Utf8Path, output_dir: &Utf8Path, path: &Utf8Path) -> Result<()> {
    let text = format!(
        r#"PROJECT_NAME = "c2rust-port-source-map"
OUTPUT_DIRECTORY = "{output_dir}"
INPUT = "{source}"
RECURSIVE = YES
EXTRACT_ALL = YES
EXTRACT_STATIC = YES
EXTRACT_LOCAL_CLASSES = YES
EXTRACT_PRIVATE = YES
FULL_PATH_NAMES = YES
STRIP_FROM_PATH = "{source}"
FILE_PATTERNS = *.c *.cc *.cpp *.cxx *.C *.h *.hh *.hpp *.hxx
EXCLUDE_PATTERNS = */.git/* */.c2rust-port/* */target/*
GENERATE_HTML = NO
GENERATE_LATEX = NO
GENERATE_XML = YES
XML_OUTPUT = xml
XML_PROGRAMLISTING = NO
HAVE_DOT = YES
CALL_GRAPH = YES
CALLER_GRAPH = YES
REFERENCES_RELATION = YES
REFERENCED_BY_RELATION = YES
QUIET = YES
WARN_IF_UNDOCUMENTED = NO
WARN_LOGFILE = "{output_dir}/warnings.log"
"#
    );
    std::fs::write(path, text).with_context(|| format!("write {path}"))
}

fn compile_database_paths(source: &Utf8Path, out: &Utf8Path) -> Result<Vec<Utf8PathBuf>> {
    let mut paths = BTreeSet::new();
    for candidate in [
        source.join("compile_commands.json"),
        out.join("raw/build-capture/bear/compile_commands.json"),
        out.join("raw/build-capture/cmake-probe/compile_commands.json"),
    ] {
        if candidate.exists() {
            paths.insert(candidate);
        }
    }
    for entry in WalkDir::new(source).into_iter().filter_map(Result::ok) {
        if !entry.file_type().is_file() {
            continue;
        }
        let path = utf8_path(entry.path())?;
        if path
            .components()
            .any(|component| component.as_str() == ".c2rust-port")
        {
            continue;
        }
        if path.file_name() == Some("compile_commands.json") {
            paths.insert(path);
        }
    }
    Ok(paths.into_iter().collect())
}

fn compile_database_units(paths: &[Utf8PathBuf]) -> Result<Vec<Utf8PathBuf>> {
    let mut units = BTreeSet::new();
    for path in paths {
        let text = std::fs::read_to_string(path).with_context(|| format!("read {path}"))?;
        let Ok(commands) = serde_json::from_str::<serde_json::Value>(&text) else {
            continue;
        };
        let Some(items) = commands.as_array() else {
            continue;
        };
        for item in items {
            if let Some(file) = item.get("file").and_then(|value| value.as_str()) {
                units.insert(Utf8PathBuf::from(file));
            }
        }
    }
    Ok(units.into_iter().collect())
}

fn is_source_or_build_file(path: &Utf8Path) -> bool {
    matches!(
        path.extension(),
        Some("c" | "cc" | "cpp" | "cxx" | "C" | "h" | "hh" | "hpp" | "hxx" | "in")
    ) || matches!(
        path.file_name(),
        Some("Makefile" | "makefile" | "CMakeLists.txt" | "configure" | "meson.build")
    )
}

fn file_role(path: &Utf8Path) -> String {
    match path.extension() {
        Some("c" | "cc" | "cpp" | "cxx" | "C") => "source",
        Some("h" | "hh" | "hpp" | "hxx") => "header",
        _ if matches!(
            path.file_name(),
            Some("Makefile" | "makefile" | "CMakeLists.txt" | "configure" | "meson.build")
        ) =>
        {
            "build"
        }
        _ => "other",
    }
    .to_string()
}

fn function_name_from_line(line: &str) -> Option<String> {
    let trimmed = line.trim();
    if trimmed.starts_with('#')
        || trimmed.ends_with(';')
        || !trimmed.contains('(')
        || !trimmed.ends_with('{')
    {
        return None;
    }
    let before_paren = trimmed.split('(').next()?.trim();
    let name = before_paren
        .split_whitespace()
        .last()?
        .trim_matches('*')
        .trim_matches('&');
    if matches!(name, "if" | "for" | "while" | "switch") || !is_function_like_identifier(name) {
        return None;
    }
    if is_macro_like_identifier(name) {
        return None;
    }
    Some(name.to_string())
}

fn is_function_like_identifier(name: &str) -> bool {
    let mut chars = name.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if !(first.is_ascii_alphabetic() || matches!(first, '_' | '~')) {
        return false;
    }
    chars.all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '_' | ':' | '~'))
}

fn is_macro_like_identifier(name: &str) -> bool {
    let leaf = name.rsplit("::").next().unwrap_or(name);
    let has_lowercase = leaf.chars().any(|ch| ch.is_ascii_lowercase());
    let has_uppercase = leaf.chars().any(|ch| ch.is_ascii_uppercase());
    if has_uppercase && !has_lowercase {
        return true;
    }
    matches!(
        leaf,
        "TEST"
            | "TEST_F"
            | "TEST_P"
            | "TYPED_TEST"
            | "ACTION"
            | "ACTION_P"
            | "ACTION_P2"
            | "ACTION_P3"
            | "ACTION_P4"
            | "ACTION_P5"
            | "ACTION_P6"
            | "ACTION_P7"
            | "ACTION_P8"
            | "ACTION_P9"
            | "ACTION_P10"
    )
}

fn rust_mirror_group(path: &Utf8Path) -> String {
    let parts = path
        .components()
        .map(|component| component.as_str())
        .collect::<Vec<_>>();
    let group = match parts.as_slice() {
        ["src", "projects", project, ..] => format!("projects/{}", rust_segment(project)),
        ["src", "common", area, ..] => format!("common/{}", rust_segment(area)),
        ["src", "test", area, ..] => format!("tests/{}", rust_segment(area)),
        ["src", area, ..] => format!("source/{}", rust_segment(area)),
        ["ext", "include", vendor, ..] | ["ext", "src", vendor, ..] => {
            format!("vendor/{}", rust_segment(vendor))
        }
        ["build_spades", ..] => "generated/build_spades".to_string(),
        [first, second, ..] => {
            format!("{}/{}", rust_segment(first), rust_file_stem_segment(second))
        }
        [first] => format!("source/{}", rust_file_stem_segment(first)),
        [] => "source/root".to_string(),
    };
    group
}

fn rust_file_stem_segment(input: &str) -> String {
    Utf8Path::new(input)
        .file_stem()
        .map(rust_segment)
        .unwrap_or_else(|| rust_segment(input))
}

fn rust_segment(input: &str) -> String {
    let mut out = String::new();
    let mut last_was_underscore = false;
    for ch in input.chars() {
        let next = if ch.is_ascii_alphanumeric() {
            ch.to_ascii_lowercase()
        } else {
            '_'
        };
        if next == '_' && last_was_underscore {
            continue;
        }
        last_was_underscore = next == '_';
        out.push(next);
    }
    let out = out.trim_matches('_');
    if out.is_empty() {
        "source".to_string()
    } else if out.as_bytes()[0].is_ascii_digit() {
        format!("n_{out}")
    } else {
        out.to_string()
    }
}

fn include_target(line: &str) -> Option<String> {
    let trimmed = line.trim();
    if !trimmed.starts_with("#include") {
        return None;
    }
    let start = trimmed.find('"')?;
    let rest = &trimmed[start + 1..];
    let end = rest.find('"')?;
    Some(rest[..end].to_string())
}

fn function_call_names_in_line<'a>(
    line: &'a str,
    known_functions: &'a BTreeSet<String>,
) -> Vec<&'a str> {
    let trimmed = line.trim();
    if trimmed.starts_with('#') {
        return Vec::new();
    }

    let mut names = Vec::new();
    for (index, _) in line.match_indices('(') {
        let prefix = &line[..index];
        let Some(name) = identifier_suffix(prefix) else {
            continue;
        };
        if is_control_keyword(name) || !known_functions.contains(name) {
            continue;
        }
        names.push(name);
    }
    names
}

fn identifier_suffix(text: &str) -> Option<&str> {
    let end = text.rfind(|ch: char| ch.is_ascii_alphanumeric() || ch == '_')? + 1;
    let start = text[..end]
        .rfind(|ch: char| !(ch.is_ascii_alphanumeric() || ch == '_'))
        .map(|index| index + 1)
        .unwrap_or(0);
    let ident = &text[start..end];
    (!ident.is_empty()).then_some(ident)
}

fn is_control_keyword(name: &str) -> bool {
    matches!(
        name,
        "if" | "for"
            | "while"
            | "switch"
            | "return"
            | "sizeof"
            | "catch"
            | "static_cast"
            | "dynamic_cast"
            | "reinterpret_cast"
            | "const_cast"
    )
}

fn mermaid_id(input: &str) -> String {
    let mut id = String::from("n_");
    for ch in input.chars() {
        if ch.is_ascii_alphanumeric() {
            id.push(ch);
        } else {
            id.push('_');
        }
    }
    id
}

fn utf8_path(path: &std::path::Path) -> Result<Utf8PathBuf> {
    Utf8PathBuf::from_path_buf(path.to_path_buf())
        .map_err(|p| anyhow::anyhow!("non-utf8 path: {}", p.display()))
}

fn sha256_hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn write_jsonl<T: Serialize>(path: &Utf8Path, rows: &[T]) -> Result<()> {
    let mut text = String::new();
    for row in rows {
        text.push_str(&serde_json::to_string(row)?);
        text.push('\n');
    }
    std::fs::write(path, text).with_context(|| format!("write {path}"))
}

fn write_jsonl_values(path: &Utf8Path, rows: &[serde_json::Value]) -> Result<()> {
    let mut text = String::new();
    for row in rows {
        text.push_str(&serde_json::to_string(row)?);
        text.push('\n');
    }
    std::fs::write(path, text).with_context(|| format!("write {path}"))
}

fn dedupe_rows_by_key_merge_provenance(rows: Vec<serde_json::Value>) -> Vec<serde_json::Value> {
    let mut by_key = BTreeMap::<String, serde_json::Value>::new();
    for row in rows {
        let key = row
            .get("key")
            .and_then(|value| value.as_str())
            .unwrap_or("")
            .to_string();
        if let Some(existing) = by_key.get_mut(&key) {
            let mut provenance = provenance_values(existing);
            provenance.extend(provenance_values(&row));
            provenance.sort();
            provenance.dedup();
            if let Some(object) = existing.as_object_mut() {
                object.insert(
                    "provenance".to_string(),
                    serde_json::Value::Array(
                        provenance
                            .into_iter()
                            .map(serde_json::Value::String)
                            .collect(),
                    ),
                );
            }
        } else {
            by_key.insert(key, row);
        }
    }
    by_key.into_values().collect()
}

fn provenance_values(row: &serde_json::Value) -> Vec<String> {
    row.get("provenance")
        .and_then(|value| value.as_array())
        .into_iter()
        .flatten()
        .filter_map(|value| value.as_str())
        .map(ToString::to_string)
        .collect()
}

fn read_lines(path: impl AsRef<std::path::Path>) -> Result<Vec<String>> {
    let path = path.as_ref();
    if !path.exists() {
        return Ok(Vec::new());
    }
    let text = std::fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
    Ok(text.lines().map(ToString::to_string).collect())
}

fn parse_compile_commands(path: &Utf8Path) -> Result<Vec<serde_json::Value>> {
    let mut rows = Vec::new();
    let text = std::fs::read_to_string(path).with_context(|| format!("read {path}"))?;
    let commands = serde_json::from_str::<serde_json::Value>(&text)
        .with_context(|| format!("parse {path}"))?;
    if let Some(items) = commands.as_array() {
        for item in items {
            let command = item
                .get("command")
                .and_then(|value| value.as_str())
                .map(ToString::to_string)
                .or_else(|| {
                    item.get("arguments").map(|value| {
                        value
                            .as_array()
                            .into_iter()
                            .flatten()
                            .filter_map(|part| part.as_str())
                            .collect::<Vec<_>>()
                            .join(" ")
                    })
                })
                .unwrap_or_default();
            rows.push(serde_json::json!({
                "fact_type": "build_unit",
                "key": sha256_hex(command.as_bytes()),
                "tool": "compile_commands.json",
                "file": item.get("file"),
                "directory": item.get("directory"),
                "command": command,
                "provenance": [path.to_string()],
            }));
        }
    }
    Ok(rows)
}

fn parse_cmake_build_units(source: &Utf8Path) -> Result<Vec<serde_json::Value>> {
    let mut rows = Vec::new();
    for cmake in build_files(source, "CMakeLists.txt")? {
        let relative = cmake.strip_prefix(source).unwrap_or(&cmake).to_path_buf();
        for (line_index, line) in read_lines(&cmake)?.into_iter().enumerate() {
            let Some((directive, target)) = parse_cmake_directive(&line) else {
                continue;
            };
            let key = format!("cmake:{relative}:{line_index}:{directive}:{target}");
            rows.push(serde_json::json!({
                "fact_type": "build_unit",
                "key": key,
                "tool": "cmake",
                "build_file": relative,
                "line_index": line_index,
                "directive": directive,
                "target": target,
                "command": line.trim(),
                "provenance": [cmake.to_string()],
            }));
        }
    }
    Ok(rows)
}

fn parse_makefile_build_units(source: &Utf8Path) -> Result<Vec<serde_json::Value>> {
    let mut rows = Vec::new();
    for makefile in build_files(source, "Makefile")? {
        let relative = makefile
            .strip_prefix(source)
            .unwrap_or(&makefile)
            .to_path_buf();
        for (line_index, line) in read_lines(&makefile)?.into_iter().enumerate() {
            let Some(target) = parse_makefile_target(&line) else {
                continue;
            };
            let key = format!("make:{relative}:{line_index}:{target}");
            rows.push(serde_json::json!({
                "fact_type": "build_unit",
                "key": key,
                "tool": "makefile",
                "build_file": relative,
                "line_index": line_index,
                "target": target,
                "command": line.trim(),
                "provenance": [makefile.to_string()],
            }));
        }
    }
    Ok(rows)
}

fn build_files(source: &Utf8Path, file_name: &str) -> Result<Vec<Utf8PathBuf>> {
    let mut paths = Vec::new();
    for entry in WalkDir::new(source).into_iter().filter_map(Result::ok) {
        if !entry.file_type().is_file() {
            continue;
        }
        let path = utf8_path(entry.path())?;
        if path
            .components()
            .any(|component| component.as_str() == ".c2rust-port")
        {
            continue;
        }
        if path.file_name() == Some(file_name) {
            paths.push(path);
        }
    }
    paths.sort();
    Ok(paths)
}

fn parse_cmake_directive(line: &str) -> Option<(&'static str, String)> {
    let line = line.split('#').next()?.trim();
    for (prefix, directive) in [
        ("add_executable(", "add_executable"),
        ("add_library(", "add_library"),
        ("target_link_libraries(", "target_link_libraries"),
        ("target_include_directories(", "target_include_directories"),
        ("add_subdirectory(", "add_subdirectory"),
        ("project(", "project"),
    ] {
        let Some(rest) = line.strip_prefix(prefix) else {
            continue;
        };
        let target = rest
            .split(|ch: char| ch == ')' || ch.is_whitespace())
            .find(|part| !part.is_empty())?;
        return Some((directive, target.trim_matches('"').to_string()));
    }
    None
}

fn parse_makefile_target(line: &str) -> Option<String> {
    if line.starts_with('\t') || line.trim_start().starts_with('#') {
        return None;
    }
    let (target, rest) = line.split_once(':')?;
    if target.trim().is_empty()
        || rest.starts_with('=')
        || target.contains('=')
        || target.contains(' ')
        || target.contains('\t')
    {
        return None;
    }
    Some(target.trim().to_string())
}

fn parse_ctags_line(line: &str) -> Option<serde_json::Value> {
    let mut parts = line.split_whitespace();
    let name = parts.next()?;
    let kind = parts.next()?;
    let line_number = parts.next()?.parse::<u64>().ok();
    let path = parts.next()?;
    let signature = line.split(path).nth(1).map(str::trim).unwrap_or("");
    Some(serde_json::json!({
        "fact_type": "symbol",
        "key": format!("{path}:{name}:{}", line_number.unwrap_or(0)),
        "name": name,
        "kind": kind,
        "path": path,
        "line": line_number,
        "signature": signature,
        "provenance": ["ctags"],
    }))
}

fn parse_clang_query_symbols(lines: &[String]) -> Vec<serde_json::Value> {
    let mut rows = Vec::new();
    let mut pending: Option<(String, u64)> = None;
    for line in lines {
        let trimmed = line.trim();
        if let Some((path, line_number)) = parse_clang_query_bind_line(trimmed) {
            pending = Some((path, line_number));
            continue;
        }
        let Some((path, line_number)) = pending.take() else {
            continue;
        };
        let Some(signature) = trimmed.split('|').nth(1).map(str::trim) else {
            pending = Some((path, line_number));
            continue;
        };
        let Some(name) = signature
            .split('(')
            .next()
            .and_then(|prefix| prefix.split_whitespace().last())
        else {
            continue;
        };
        rows.push(serde_json::json!({
            "fact_type": "symbol",
            "key": format!("{path}:{name}:{line_number}:clang-query"),
            "name": name.trim_matches('*'),
            "kind": "function",
            "path": path,
            "line": line_number,
            "signature": signature,
            "provenance": ["clang-query"],
        }));
    }
    rows
}

fn parse_clang_query_bind_line(line: &str) -> Option<(String, u64)> {
    if !line.contains("note: \"function\" binds here") {
        return None;
    }
    let location = line.split(": note:").next()?;
    let mut parts = location.rsplitn(3, ':');
    let _column = parts.next()?;
    let line_number = parts.next()?;
    let path = parts.next()?;
    Some((path.to_string(), line_number.parse().ok()?))
}

fn parse_cflow_edges_from_path(path: &Utf8Path, max_rows: usize) -> Result<Vec<serde_json::Value>> {
    let file = std::fs::File::open(path).with_context(|| format!("open {path}"))?;
    let reader = std::io::BufReader::new(file);
    let mut rows = Vec::new();
    let mut stack: Vec<(usize, String)> = Vec::new();
    let mut truncated = false;
    for line in reader.lines() {
        let line = line.with_context(|| format!("read {path}"))?;
        let Some(row) = parse_cflow_line(&line, &mut stack) else {
            continue;
        };
        if rows.len() >= max_rows {
            truncated = true;
            continue;
        }
        rows.push(row);
    }
    if truncated {
        rows.push(serde_json::json!({
            "fact_type": "call_edge",
            "key": format!("cflow:truncated:{max_rows}"),
            "caller": "cflow-normalizer",
            "callee": "remaining raw cflow edges",
            "source_span": null,
            "evidence": format!("normalized cflow rows truncated after {max_rows}; raw cflow output remains preserved"),
            "provenance": ["cflow"],
        }));
    }
    Ok(rows)
}

fn parse_cflow_line(line: &str, stack: &mut Vec<(usize, String)>) -> Option<serde_json::Value> {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return None;
    }
    let level = line.chars().take_while(|ch| ch.is_whitespace()).count() / 4;
    let name = trimmed
        .split('(')
        .next()
        .map(str::trim)
        .filter(|name| !name.is_empty())?;
    while stack
        .last()
        .is_some_and(|(prior_level, _)| *prior_level >= level)
    {
        stack.pop();
    }
    let row = stack.last().map(|(_, caller)| {
        serde_json::json!({
            "fact_type": "call_edge",
            "key": format!("{caller}->{name}:cflow"),
            "caller": caller,
            "callee": name,
            "source_span": trimmed,
            "evidence": "cflow",
            "provenance": ["cflow"],
        })
    });
    stack.push((level, name.to_string()));
    row
}

fn normalize_codeql_sarif_diagnostics(out: &Utf8Path) -> Result<Vec<serde_json::Value>> {
    let mut rows = Vec::new();
    for path in find_paths(out.join("raw/semantic-analysis/codeql"), Some("sarif"))? {
        let text = std::fs::read_to_string(&path).with_context(|| format!("read {path}"))?;
        let Ok(sarif) = serde_json::from_str::<serde_json::Value>(&text) else {
            continue;
        };
        let Some(runs) = sarif.get("runs").and_then(|value| value.as_array()) else {
            continue;
        };
        for (run_index, run) in runs.iter().enumerate() {
            let Some(results) = run.get("results").and_then(|value| value.as_array()) else {
                continue;
            };
            for (result_index, result) in results.iter().enumerate() {
                let message = result
                    .get("message")
                    .and_then(|value| value.get("text"))
                    .and_then(|value| value.as_str())
                    .unwrap_or("");
                let rule_id = result
                    .get("ruleId")
                    .and_then(|value| value.as_str())
                    .unwrap_or("unknown");
                rows.push(serde_json::json!({
                    "fact_type": "diagnostic",
                    "key": format!("codeql:{run_index}:{result_index}:{rule_id}"),
                    "tool": "codeql",
                    "rule_id": rule_id,
                    "message": message,
                    "locations": result.get("locations"),
                    "provenance": [path.to_string()],
                }));
            }
        }
    }
    Ok(rows)
}

fn find_paths(root: Utf8PathBuf, extension: Option<&str>) -> Result<Vec<Utf8PathBuf>> {
    if !root.exists() {
        return Ok(Vec::new());
    }
    let mut paths = Vec::new();
    for entry in WalkDir::new(&root).into_iter().filter_map(Result::ok) {
        let path = utf8_path(entry.path())?;
        if let Some(extension) = extension {
            if entry.file_type().is_file() && path.extension() == Some(extension) {
                paths.push(path);
            }
        } else if entry.file_type().is_dir() {
            paths.push(path);
        }
    }
    paths.sort();
    Ok(paths)
}

fn timestamp_slug() -> String {
    let now = Utc::now();
    format!(
        "{}-{}",
        now.format("%Y%m%dT%H%M%SZ"),
        now.timestamp_subsec_nanos()
    )
}

fn discover_codeql_cpp_query_spec() -> Option<String> {
    for spec in ["cpp-code-scanning.qls", "codeql/cpp-queries"] {
        let output = Command::new("codeql")
            .args(["resolve", "queries", spec])
            .output()
            .ok()?;
        if output.status.success() {
            return Some(spec.to_string());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strategy_has_exhaustive_consolidation_tables() {
        let tables = fact_tables();
        assert!(tables.iter().any(|table| table.name == "capabilities"));
        assert!(tables.iter().any(|table| table.name == "symbols"));
        assert!(tables.iter().any(|table| table.name == "types"));
        assert!(tables.iter().any(|table| table.name == "call_edges"));
        assert!(tables.iter().any(|table| table.name == "dataflow_edges"));
        assert!(tables.iter().any(|table| table.name == "equivalence_edges"));
        assert!(tables.iter().any(|table| table.name == "equivalence_diffs"));
        assert!(tables.iter().any(|table| table.name == "runtime_events"));
        assert!(tables.iter().any(|table| table.name == "rust_workspace"));
    }

    #[test]
    fn strategy_includes_repomix_bundle_stage() {
        let stages = stages();
        assert!(stages
            .iter()
            .any(|stage| stage.name == "bundle" && stage.tools.contains(&"repomix".to_string())));
    }

    #[test]
    fn doxygen_xml_normalizer_extracts_docs_types_and_references() {
        let dir = camino::Utf8PathBuf::from_path_buf(std::env::temp_dir())
            .unwrap()
            .join(format!("c2rust-port-doxygen-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let xml_path = dir.join("classExample.xml");
        std::fs::write(
            &xml_path,
            r#"<doxygen>
  <compounddef id="classExample" kind="class">
    <compoundname>Example</compoundname>
    <briefdescription><para>Class docs.</para></briefdescription>
    <sectiondef>
      <memberdef id="classExample_1a" kind="function">
        <type>int</type>
        <definition>int Example::lookup</definition>
        <argsstring>(int key)</argsstring>
        <name>lookup</name>
        <briefdescription><para>Lookup docs.</para></briefdescription>
        <location file="src/index.cpp" line="42"/>
        <references refid="classExample_1b">probe</references>
      </memberdef>
    </sectiondef>
  </compounddef>
</doxygen>"#,
        )
        .unwrap();
        let mut facts = DoxygenFacts::default();
        parse_doxygen_xml_file(&xml_path, &mut facts).unwrap();
        assert!(facts.symbols.iter().any(|row| {
            row.get("name").and_then(|value| value.as_str()) == Some("lookup")
                && row
                    .get("doc_comment")
                    .and_then(|value| value.as_str())
                    .is_some_and(|text| text.contains("Lookup docs"))
        }));
        assert!(facts.types.iter().any(|row| {
            row.get("qualified_name").and_then(|value| value.as_str()) == Some("Example")
        }));
        assert!(facts.call_edges.iter().any(|row| {
            row.get("caller").and_then(|value| value.as_str()) == Some("Example::lookup")
                && row.get("callee").and_then(|value| value.as_str()) == Some("probe")
        }));
        let _ = std::fs::remove_dir_all(dir);
    }
}
