use anyhow::{Context, Result};
use camino::{Utf8Path, Utf8PathBuf};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::process::Command;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BenchmarkManifest {
    pub dataset_id: String,
    pub subset: String,
    pub reads: u64,
    pub reference_path: Option<Utf8PathBuf>,
    pub read_files: Vec<Utf8PathBuf>,
    pub subset_command: Option<String>,
    pub source_command: Option<String>,
    pub expected_outputs: Vec<Utf8PathBuf>,
    pub hashes: Vec<String>,
    pub parser: String,
}

#[derive(Debug, Serialize)]
struct RunRecord {
    timestamp: chrono::DateTime<Utc>,
    command: String,
    status: String,
    stdout: String,
    stderr: String,
}

const SUBSETS: &[(&str, u64)] = &[
    ("tiny", 100),
    ("smoke", 1_000),
    ("medium", 10_000),
    ("large", 100_000),
];

pub fn prepare(source: &Utf8Path) -> Result<()> {
    let dir = source.join(".c2rust-port/bench/manifests");
    std::fs::create_dir_all(&dir).with_context(|| format!("create {dir}"))?;
    for (name, reads) in SUBSETS {
        let manifest = BenchmarkManifest {
            dataset_id: format!("{}-{name}", source.file_name().unwrap_or("source")),
            subset: (*name).to_string(),
            reads: *reads,
            reference_path: None,
            read_files: Vec::new(),
            subset_command: Some(format!(
                "seqtk sample -s100 <reads.fastq> {reads} > {name}.fastq"
            )),
            source_command: None,
            expected_outputs: Vec::new(),
            hashes: Vec::new(),
            parser: "semantic-summary-v1".to_string(),
        };
        manifest.write_to(&dir.join(format!("{name}.json")))?;
    }
    println!("wrote benchmark manifests to {dir}");
    Ok(())
}

pub fn run_source(source: &Utf8Path) -> Result<()> {
    let dir = source.join(".c2rust-port/bench/runs");
    std::fs::create_dir_all(&dir).with_context(|| format!("create {dir}"))?;
    let record = run_instrumentation_probe(source);
    let path = dir.join(format!(
        "source-{}.jsonl",
        Utc::now().format("%Y%m%dT%H%M%SZ")
    ));
    std::fs::write(&path, format!("{}\n", serde_json::to_string(&record)?))
        .with_context(|| format!("write {path}"))?;
    println!("wrote source benchmark run evidence to {path}");
    Ok(())
}

fn run_instrumentation_probe(source: &Utf8Path) -> RunRecord {
    let command = if source.join("Makefile").exists() {
        "make -n".to_string()
    } else if source.join("CMakeLists.txt").exists() {
        "cmake -S . -B /tmp/c2rust-port-cmake-probe".to_string()
    } else {
        "no supported source build command detected".to_string()
    };
    if command.starts_with("no supported") {
        return RunRecord {
            timestamp: Utc::now(),
            command,
            status: "unsupported".to_string(),
            stdout: String::new(),
            stderr: "no Makefile or CMakeLists.txt found; configure source_command in manifest"
                .to_string(),
        };
    }
    let output = Command::new("sh")
        .arg("-c")
        .arg(&command)
        .current_dir(source)
        .output();
    match output {
        Ok(output) => RunRecord {
            timestamp: Utc::now(),
            command,
            status: if output.status.success() {
                "ok"
            } else {
                "failed"
            }
            .to_string(),
            stdout: String::from_utf8_lossy(&output.stdout).to_string(),
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
        },
        Err(err) => RunRecord {
            timestamp: Utc::now(),
            command,
            status: "unsupported".to_string(),
            stdout: String::new(),
            stderr: err.to_string(),
        },
    }
}

impl BenchmarkManifest {
    pub fn write_to(&self, path: &Utf8Path) -> Result<()> {
        std::fs::write(path, serde_json::to_string_pretty(self)?)
            .with_context(|| format!("write {path}"))
    }

    pub fn read_from(path: &Utf8Path) -> Result<Self> {
        let text = std::fs::read_to_string(path).with_context(|| format!("read {path}"))?;
        serde_json::from_str(&text).with_context(|| format!("parse {path}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn benchmark_manifest_round_trips() {
        let dir = camino::Utf8PathBuf::from_path_buf(
            std::env::temp_dir().join(format!("c2rust-port-test-{}", std::process::id())),
        )
        .unwrap();
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("manifest.json");
        let manifest = BenchmarkManifest {
            dataset_id: "fixture".to_string(),
            subset: "tiny".to_string(),
            reads: 100,
            reference_path: None,
            read_files: vec!["reads.fastq".into()],
            subset_command: None,
            source_command: Some("make test".to_string()),
            expected_outputs: vec![],
            hashes: vec![],
            parser: "parser".to_string(),
        };
        manifest.write_to(&path).unwrap();
        assert_eq!(BenchmarkManifest::read_from(&path).unwrap(), manifest);
        let _ = std::fs::remove_file(path);
        let _ = std::fs::remove_dir(dir);
    }
}
