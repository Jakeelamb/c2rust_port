use anyhow::{Context, Result};
use camino::{Utf8Path, Utf8PathBuf};
use chrono::Utc;
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::process::Command;
use walkdir::WalkDir;

const TOOLS: &[ToolSpec] = &[
    ToolSpec::new(
        "repomix",
        "repo bundling",
        "final source and fact bundle for agent context",
    ),
    ToolSpec::new(
        "clang",
        "C/C++ mapping",
        "preprocessing, AST, diagnostics, compile checks",
    ),
    ToolSpec::new(
        "clang++",
        "C/C++ mapping",
        "C++ compile checks and template-heavy source validation",
    ),
    ToolSpec::new(
        "clang-tidy",
        "C/C++ mapping",
        "static diagnostics and mechanical modernization hints",
    ),
    ToolSpec::new(
        "clang-query",
        "C/C++ mapping",
        "interactive AST queries for API and type discovery",
    ),
    ToolSpec::new(
        "clangd",
        "C/C++ mapping",
        "language server index, references, symbols, and xrefs",
    ),
    ToolSpec::new(
        "llvm-cov",
        "C/C++ tracing",
        "LLVM source coverage when available",
    ),
    ToolSpec::new(
        "llvm-profdata",
        "C/C++ tracing",
        "LLVM profile data merge and summaries",
    ),
    ToolSpec::new(
        "bear",
        "C/C++ build capture",
        "compile_commands.json generation from builds",
    ),
    ToolSpec::new(
        "intercept-build",
        "C/C++ build capture",
        "scan-build compiler interception",
    ),
    ToolSpec::new(
        "compiledb",
        "C/C++ build capture",
        "compile database generation",
    ),
    ToolSpec::new(
        "cmake",
        "C/C++ build capture",
        "CMake configure and compile database export",
    ),
    ToolSpec::new(
        "make",
        "C/C++ build capture",
        "Makefile build and dry-run tracing",
    ),
    ToolSpec::new(
        "ninja",
        "C/C++ build capture",
        "Ninja build graph and command tracing",
    ),
    ToolSpec::new(
        "meson",
        "C/C++ build capture",
        "Meson configure and introspection",
    ),
    ToolSpec::new(
        "pkg-config",
        "C/C++ build capture",
        "native dependency flags and link metadata",
    ),
    ToolSpec::new("ctags", "C/C++ mapping", "symbol inventory"),
    ToolSpec::new("cflow", "C/C++ mapping", "C call graph extraction"),
    ToolSpec::new(
        "cscope",
        "C/C++ mapping",
        "symbol and caller/callee database",
    ),
    ToolSpec::new(
        "doxygen",
        "C/C++ mapping",
        "documentation and XML structure extraction",
    ),
    ToolSpec::new("joern", "C/C++ mapping", "code property graph analysis"),
    ToolSpec::new(
        "joern-parse",
        "C/C++ mapping",
        "Joern code property graph creation",
    ),
    ToolSpec::new(
        "codeql",
        "C/C++ mapping",
        "semantic database and query analysis",
    ),
    ToolSpec::new(
        "strace",
        "source runtime tracing",
        "syscall and file access tracing",
    ),
    ToolSpec::new(
        "ltrace",
        "source runtime tracing",
        "dynamic library call tracing",
    ),
    ToolSpec::new(
        "perf",
        "runtime tracing",
        "sampled CPU profiles and call graphs",
    ),
    ToolSpec::new(
        "valgrind",
        "runtime tracing",
        "memcheck and callgrind for small workloads",
    ),
    ToolSpec::new(
        "callgrind_annotate",
        "runtime tracing",
        "callgrind profile summaries",
    ),
    ToolSpec::new("gprof", "C/C++ tracing", "-pg function timing reports"),
    ToolSpec::new("gcov", "C/C++ tracing", "GCC coverage reports"),
    ToolSpec::new(
        "lcov",
        "C/C++ tracing",
        "coverage capture and HTML summaries",
    ),
    ToolSpec::new("rr", "runtime tracing", "record/replay debugging"),
    ToolSpec::new(
        "gdb",
        "runtime tracing",
        "batch stack inspection and breakpoints",
    ),
    ToolSpec::new("lldb", "runtime tracing", "LLVM debugger stack inspection"),
    ToolSpec::new("bpftrace", "runtime tracing", "kernel/user probe tracing"),
    ToolSpec::new("hyperfine", "runtime tracing", "repeatable command timing"),
    ToolSpec::new("time", "runtime tracing", "wall clock and RSS summaries"),
    ToolSpec::new("seqtk", "benchmark corpus", "FASTQ subset generation"),
    ToolSpec::new(
        "samtools",
        "benchmark corpus",
        "SAM/BAM inspection and normalization",
    ),
    ToolSpec::new(
        "cargo",
        "Rust mapping",
        "build graph, metadata, tests, and execution",
    ),
    ToolSpec::new(
        "rustc",
        "Rust mapping",
        "compiler diagnostics and expanded configuration",
    ),
    ToolSpec::new("rustdoc", "Rust mapping", "public API extraction and docs"),
    ToolSpec::new("rustfmt", "Rust mapping", "format gate"),
    ToolSpec::new(
        "clippy-driver",
        "Rust mapping",
        "lint diagnostics behind cargo clippy",
    ),
    ToolSpec::new(
        "rust-analyzer",
        "Rust mapping",
        "symbols, xrefs, and semantic model",
    ),
    ToolSpec::new(
        "cargo-metadata",
        "Rust mapping",
        "workspace/package metadata when installed as subcommand",
    ),
    ToolSpec::new("cargo-expand", "Rust mapping", "macro expansion"),
    ToolSpec::new("cargo-modules", "Rust mapping", "module tree extraction"),
    ToolSpec::new("cargo-udeps", "Rust mapping", "unused dependency detection"),
    ToolSpec::new(
        "cargo-deny",
        "Rust mapping",
        "dependency/license/advisory graph",
    ),
    ToolSpec::new("cargo-nextest", "Rust tracing", "structured test execution"),
    ToolSpec::new("cargo-llvm-cov", "Rust tracing", "Rust coverage"),
    ToolSpec::new("cargo-flamegraph", "Rust tracing", "Rust perf flamegraphs"),
    ToolSpec::new("cargo-profiler", "Rust tracing", "Rust profiling wrapper"),
    ToolSpec::new("cargo-bloat", "Rust tracing", "binary size attribution"),
    ToolSpec::new("cargo-asm", "Rust tracing", "assembly output for hot paths"),
    ToolSpec::new(
        "cargo-instruments",
        "Rust tracing",
        "macOS Instruments wrapper where applicable",
    ),
    ToolSpec::new("heaptrack", "runtime tracing", "heap allocation profiling"),
    ToolSpec::new(
        "dh-atop",
        "runtime tracing",
        "system resource snapshots when available",
    ),
];

#[derive(Debug, Clone, Copy)]
struct ToolSpec {
    name: &'static str,
    category: &'static str,
    purpose: &'static str,
}

impl ToolSpec {
    const fn new(name: &'static str, category: &'static str, purpose: &'static str) -> Self {
        Self {
            name,
            category,
            purpose,
        }
    }
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ToolStatus {
    pub name: String,
    pub category: String,
    pub purpose: String,
    pub installed: bool,
    pub path: Option<String>,
}

#[derive(Debug, Serialize)]
struct BuildSystem {
    has_makefile: bool,
    has_cmake: bool,
    has_compile_commands: bool,
}

#[derive(Debug, Serialize)]
struct SourceFile {
    path: Utf8PathBuf,
    bytes: u64,
    sha256: String,
}

#[derive(Debug, Serialize)]
struct Entrypoints {
    likely_mains: Vec<Utf8PathBuf>,
}

#[derive(Debug, Serialize)]
struct DiagnosticRun {
    timestamp: chrono::DateTime<Utc>,
    tool: String,
    status: String,
    detail: String,
}

pub fn run(source: &Utf8Path) -> Result<()> {
    let out = source.join(".c2rust-port/inspect");
    std::fs::create_dir_all(&out).with_context(|| format!("create {out}"))?;

    write_json(&out.join("tool-audit.json"), &audit_tools())?;
    write_json(&out.join("build-system.json"), &build_system(source))?;
    let inventory = source_inventory(source)?;
    write_json(&out.join("source-inventory.json"), &inventory)?;
    write_json(&out.join("entrypoints.json"), &entrypoints(&inventory))?;
    write_jsonl(&out.join("diagnostic-runs.jsonl"), &[] as &[DiagnosticRun])?;
    println!("wrote inspection artifacts to {out}");
    Ok(())
}

pub fn audit_tools() -> Vec<ToolStatus> {
    TOOLS.iter().map(audit_tool).collect()
}

fn audit_tool(spec: &ToolSpec) -> ToolStatus {
    let (installed, path) = if let Some(subcommand) = spec.name.strip_prefix("cargo-") {
        probe_cargo_subcommand(subcommand)
    } else {
        probe_executable(spec.name)
    };

    ToolStatus {
        name: spec.name.to_string(),
        category: spec.category.to_string(),
        purpose: spec.purpose.to_string(),
        installed,
        path,
    }
}

fn probe_executable(name: &str) -> (bool, Option<String>) {
    match Command::new("sh")
        .arg("-c")
        .arg(format!("command -v {name}"))
        .output()
    {
        Ok(output) if output.status.success() => (
            true,
            Some(String::from_utf8_lossy(&output.stdout).trim().to_string()),
        ),
        _ => (false, None),
    }
}

fn probe_cargo_subcommand(subcommand: &str) -> (bool, Option<String>) {
    let (has_cargo, cargo_path) = probe_executable("cargo");
    if !has_cargo {
        return (false, None);
    }
    match Command::new("cargo").arg(subcommand).arg("--help").output() {
        Ok(output) if output.status.success() => (true, cargo_path),
        _ => (false, cargo_path),
    }
}

fn build_system(source: &Utf8Path) -> BuildSystem {
    BuildSystem {
        has_makefile: source.join("Makefile").exists() || source.join("makefile").exists(),
        has_cmake: source.join("CMakeLists.txt").exists(),
        has_compile_commands: source.join("compile_commands.json").exists(),
    }
}

fn source_inventory(source: &Utf8Path) -> Result<Vec<SourceFile>> {
    let mut files = Vec::new();
    for entry in WalkDir::new(source).into_iter().filter_map(Result::ok) {
        if !entry.file_type().is_file() {
            continue;
        }
        let path = Utf8PathBuf::from_path_buf(entry.path().to_path_buf())
            .map_err(|p| anyhow::anyhow!("non-utf8 path: {}", p.display()))?;
        if is_source_file(&path) {
            let bytes = std::fs::read(&path).with_context(|| format!("read {path}"))?;
            files.push(SourceFile {
                path: path.strip_prefix(source).unwrap_or(&path).to_path_buf(),
                bytes: bytes.len() as u64,
                sha256: format!("{:x}", Sha256::digest(&bytes)),
            });
        }
    }
    files.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(files)
}

fn entrypoints(files: &[SourceFile]) -> Entrypoints {
    Entrypoints {
        likely_mains: files
            .iter()
            .filter(|f| f.path.file_stem().is_some_and(|s| s == "main"))
            .map(|f| f.path.clone())
            .collect(),
    }
}

fn is_source_file(path: &Utf8Path) -> bool {
    matches!(
        path.extension(),
        Some("c" | "cc" | "cpp" | "cxx" | "h" | "hh" | "hpp" | "hxx")
    )
}

fn write_json<T: Serialize>(path: &Utf8Path, value: &T) -> Result<()> {
    std::fs::write(path, serde_json::to_string_pretty(value)?)
        .with_context(|| format!("write {path}"))
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
    fn missing_tool_parser_reports_false() {
        let status = audit_tool(&ToolSpec::new(
            "definitely-not-a-real-c2rust-port-tool",
            "test",
            "test",
        ));
        assert!(!status.installed);
        assert_eq!(status.path, None);
    }

    #[test]
    fn audit_covers_c_and_rust_mapping_and_tracing() {
        let statuses = audit_tools();
        assert!(statuses.iter().any(|s| s.category == "C/C++ mapping"));
        assert!(statuses.iter().any(|s| s.category == "C/C++ tracing"));
        assert!(statuses.iter().any(|s| s.category == "Rust mapping"));
        assert!(statuses.iter().any(|s| s.category == "Rust tracing"));
    }
}
