use anyhow::{Context, Result};
use camino::{Utf8Path, Utf8PathBuf};
use chrono::Utc;
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
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

pub fn run(source: &Utf8Path, target: &Utf8Path) -> Result<()> {
    let out = source.join(".c2rust-port/knowledge");
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
    write_json(&out.join("repo-map.json"), &repo_map)?;
    write_repo_map_markdown(&out.join("repo-map.md"), &repo_map)?;
    write_mirror_docs(target, &repo_map)?;
    write_full_picture(&out.join("bundles/full-picture.md"), &strategy, &repo_map)?;
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
                "clang",
                "clang++",
                "clang-query",
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
            &["clang-tidy", "joern", "codeql"],
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
            &["clang", "clang-tidy", "codeql", "cargo", "clippy-driver"],
            "Compile, lint, semantic, and security findings.",
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
    let compile_units = compile_units(&source_files);

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
    if installed.contains("clang") && !compile_units.is_empty() {
        let mut args = vec!["clang".to_string(), "-fsyntax-only".to_string()];
        args.extend(compile_units.iter().take(64).map(|file| file.to_string()));
        runs.push(run_capture_owned(
            source,
            out,
            "semantic-analysis",
            "clang",
            &args,
            "syntax-only compiler diagnostics over bounded source set",
        )?);
    }
    if installed.contains("clang-tidy") && !compile_units.is_empty() {
        let mut args = vec![
            "clang-tidy".to_string(),
            "--quiet".to_string(),
            "-checks=clang-diagnostic-*,bugprone-*,performance-*".to_string(),
        ];
        args.extend(compile_units.iter().take(32).map(|file| file.to_string()));
        args.push("--".to_string());
        args.push("-I.".to_string());
        runs.push(run_capture_owned(
            source,
            out,
            "semantic-analysis",
            "clang-tidy",
            &args,
            "diagnostic checks over bounded source set",
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
        for (name, target_file) in functions {
            if file.path == *target_file {
                continue;
            }
            if has_call_site(&text, name) {
                edges.push(DataFlowEdge {
                    from: file.path.to_string(),
                    to: format!("function:{name}"),
                    evidence: "call-site heuristic".to_string(),
                });
            }
        }
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
            let stem = file.path.file_stem().unwrap_or("source").replace('-', "_");
            grouped.entry(stem).or_default().push(file.path.clone());
        }
    }
    let modules = grouped
        .into_iter()
        .map(|(stem, mirrors)| RustModulePlan {
            rust_path: target.join(format!("src/{stem}.rs")),
            mirrors,
            reason: "one Rust module per source/header ownership cluster".to_string(),
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
) -> Result<()> {
    let mut text = String::new();
    text.push_str("# Full Repo Picture\n\n");
    text.push_str(&format!("- Goal: {}\n", strategy.goal));
    text.push_str(&format!("- Source: `{}`\n", repo_map.source_repo));
    text.push_str(&format!("- Target: `{}`\n\n", repo_map.target_repo));
    text.push_str("## Process Flow\n\n");
    for step in &repo_map.process_flow {
        text.push_str(&format!("- `{}`: `{}`\n", step.kind, step.label));
    }
    text.push_str("\n## Data Flow\n\n");
    for edge in &repo_map.data_flow {
        text.push_str(&format!(
            "- `{}` -> `{}` via {}\n",
            edge.from, edge.to, edge.evidence
        ));
    }
    text.push_str("\n## Rust Mirror\n\n");
    for module in &repo_map.rust_mirror.modules {
        text.push_str(&format!("- `{}`\n", module.rust_path));
    }
    text.push_str("\n## Fact Tables\n\n");
    for table in &strategy.fact_tables {
        text.push_str(&format!(
            "- `{}` keyed by `{}`: {}\n",
            table.name, table.record_key, table.purpose
        ));
    }
    std::fs::write(path, text).with_context(|| format!("write {path}"))
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

fn compile_units(files: &[Utf8PathBuf]) -> Vec<Utf8PathBuf> {
    files
        .iter()
        .filter(|path| matches!(path.extension(), Some("c" | "cc" | "cpp" | "cxx" | "C")))
        .cloned()
        .collect()
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
    let name = before_paren.split_whitespace().last()?;
    if matches!(name, "if" | "for" | "while" | "switch") {
        return None;
    }
    Some(name.trim_matches('*').to_string())
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

fn has_call_site(text: &str, name: &str) -> bool {
    let needle = format!("{name}(");
    text.lines().any(|line| {
        let trimmed = line.trim();
        !trimmed.starts_with('#') && trimmed.contains(&needle)
    })
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
