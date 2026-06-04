use anyhow::{Context, Result};
use camino::{Utf8Path, Utf8PathBuf};
use chrono::Utc;
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
    let repo_map = build_repo_map(source, target)?;
    normalize_facts(source, target, &out, &repo_map, &evidence_runs)?;
    write_json(&out.join("repo-map.json"), &repo_map)?;
    write_repo_map_markdown(&out.join("repo-map.md"), &repo_map)?;
    write_mirror_docs(target, &repo_map)?;
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
                "repo-system-map",
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
            "files",
            "path",
            &["repo-system-map", "walkdir", "repomix"],
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
        dedupe(
            "files",
            "path",
            &["repo-system-map", "source-inventory", "repomix"],
        ),
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
        "files",
        "build_units",
        "symbols",
        "call_edges",
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
    repo_map: &SourceRepoMap,
    evidence_runs: &[EvidenceRun],
) -> Result<()> {
    write_jsonl_values(
        &out.join("facts/files.jsonl"),
        &normalize_file_facts(&repo_map.files),
    )?;
    write_jsonl_values(
        &out.join("facts/build_units.jsonl"),
        &normalize_build_units(source, out, evidence_runs)?,
    )?;
    write_jsonl_values(
        &out.join("facts/symbols.jsonl"),
        &normalize_symbols(out, repo_map)?,
    )?;
    write_jsonl_values(
        &out.join("facts/call_edges.jsonl"),
        &normalize_call_edges(out, repo_map)?,
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

fn normalize_symbols(out: &Utf8Path, repo_map: &SourceRepoMap) -> Result<Vec<serde_json::Value>> {
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
    rows.sort_by_key(|row| {
        row.get("key")
            .and_then(|value| value.as_str())
            .unwrap_or("")
            .to_string()
    });
    rows.dedup_by(|left, right| left.get("key") == right.get("key"));
    Ok(rows)
}

fn normalize_call_edges(
    out: &Utf8Path,
    repo_map: &SourceRepoMap,
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
    rows.sort_by_key(|row| {
        row.get("key")
            .and_then(|value| value.as_str())
            .unwrap_or("")
            .to_string()
    });
    rows.dedup_by(|left, right| left.get("key") == right.get("key"));
    Ok(rows)
}

fn normalize_diagnostics(
    _out: &Utf8Path,
    evidence_runs: &[EvidenceRun],
) -> Result<Vec<serde_json::Value>> {
    let mut rows = Vec::new();
    for run in evidence_runs.iter().filter(|run| {
        matches!(
            run.tool.as_str(),
            "clang-tidy" | "cargo-check" | "codeql-create" | "codeql-analyze" | "joern-parse"
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
            "clang-query" | "codeql-create" | "codeql-analyze" | "joern-parse"
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
    for table in &strategy.fact_tables {
        text.push_str(&format!(
            "- `facts/{}.jsonl`: {} rows. {}\n",
            table.name,
            count_jsonl_rows(&out.join(format!("facts/{}.jsonl", table.name)))?,
            table.purpose
        ));
    }
    text.push_str("\n## Raw Evidence\n\n");
    text.push_str("- Tool run ledger: `raw/evidence-runs.jsonl`.\n");
    text.push_str("- Source structure: `raw/source-structure/`.\n");
    text.push_str("- Semantic analysis: `raw/semantic-analysis/`.\n");
    text.push_str("- Build capture: `raw/build-capture/`.\n");
    text.push_str("- Rust target: `raw/rust-target/`.\n");
    std::fs::write(path, text).with_context(|| format!("write {path}"))
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
        assert!(tables.iter().any(|table| table.name == "symbols"));
        assert!(tables.iter().any(|table| table.name == "call_edges"));
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
}
