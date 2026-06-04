use anyhow::{Context, Result};
use camino::{Utf8Path, Utf8PathBuf};
use chrono::Utc;
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::process::Command;
use walkdir::WalkDir;

const TOOLS: &[&str] = &[
    "clang",
    "clang++",
    "clang-tidy",
    "clang-query",
    "clangd",
    "bear",
    "ctags",
    "cflow",
    "joern",
    "codeql",
    "perf",
    "valgrind",
    "gprof",
    "gcov",
    "rr",
    "cargo",
    "cargo-flamegraph",
    "cargo-llvm-cov",
    "seqtk",
];

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ToolStatus {
    pub name: String,
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
    write_jsonl(
        &out.join("diagnostic-runs.jsonl"),
        &[
            repo_system_map_run("rewrite-prep", source),
            repo_system_map_run("semantic-export", source),
        ],
    )?;
    println!("wrote inspection artifacts to {out}");
    Ok(())
}

pub fn audit_tools() -> Vec<ToolStatus> {
    TOOLS.iter().map(|tool| audit_tool(tool)).collect()
}

pub fn audit_tool(name: &str) -> ToolStatus {
    let exe = if name == "cargo-flamegraph" {
        "cargo"
    } else if name == "cargo-llvm-cov" {
        "cargo"
    } else {
        name
    };
    let output = Command::new("sh")
        .arg("-c")
        .arg(format!("command -v {exe}"))
        .output();
    match output {
        Ok(output) if output.status.success() => ToolStatus {
            name: name.to_string(),
            installed: true,
            path: Some(String::from_utf8_lossy(&output.stdout).trim().to_string()),
        },
        _ => ToolStatus {
            name: name.to_string(),
            installed: false,
            path: None,
        },
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

fn repo_system_map_run(kind: &str, source: &Utf8Path) -> DiagnosticRun {
    let args: &[&str] = match kind {
        "rewrite-prep" => &[
            "rewrite-prep",
            source.as_str(),
            "--source",
            "auto",
            "--target",
            "rust",
        ],
        "semantic-export" => &[
            "semantic-export",
            source.as_str(),
            "--tool",
            "clang",
            "--emit",
            "all",
        ],
        _ => &[],
    };
    let output = Command::new("repo-system-map").args(args).output();
    match output {
        Ok(output) => DiagnosticRun {
            timestamp: Utc::now(),
            tool: format!("repo-system-map {kind}"),
            status: if output.status.success() {
                "ok"
            } else {
                "failed"
            }
            .to_string(),
            detail: String::from_utf8_lossy(&output.stderr).trim().to_string(),
        },
        Err(err) => DiagnosticRun {
            timestamp: Utc::now(),
            tool: format!("repo-system-map {kind}"),
            status: "unsupported".to_string(),
            detail: err.to_string(),
        },
    }
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
        let status = audit_tool("definitely-not-a-real-c2rust-port-tool");
        assert!(!status.installed);
        assert_eq!(status.path, None);
    }
}
