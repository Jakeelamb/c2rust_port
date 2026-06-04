use anyhow::{Context, Result};
use camino::Utf8Path;
use serde::Serialize;

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
