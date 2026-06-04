use anyhow::{Context, Result, bail};
use camino::{Utf8Path, Utf8PathBuf};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LocalConfig {
    pub benchmark_root: Option<Utf8PathBuf>,
    pub biological_data_root: Option<Utf8PathBuf>,
    pub repo_system_map: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PacketProfile {
    pub name: String,
    pub max_source_files: usize,
    pub max_map_rows: usize,
    pub max_prompt_bytes: usize,
    pub allow_verification: bool,
}

impl Default for PacketProfile {
    fn default() -> Self {
        Self {
            name: "translator-default".to_string(),
            max_source_files: 3,
            max_map_rows: 80,
            max_prompt_bytes: 24_000,
            allow_verification: false,
        }
    }
}

impl PacketProfile {
    pub fn write_to(&self, path: &Utf8Path) -> Result<()> {
        let text = toml::to_string_pretty(self).context("serialize packet profile")?;
        std::fs::write(path, text).with_context(|| format!("write {path}"))
    }

    pub fn read_from(path: &Utf8Path) -> Result<Self> {
        let text = std::fs::read_to_string(path).with_context(|| format!("read {path}"))?;
        toml::from_str(&text).with_context(|| format!("parse {path}"))
    }
}

pub fn reject_public_default_paths(toml_text: &str) -> Result<()> {
    let disallowed = ["/home/jake", "/Users/", "C:\\Users\\"];
    for needle in disallowed {
        if toml_text.contains(needle) {
            bail!("public config contains local absolute path: {needle}");
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn public_safe_config_rejects_local_paths() {
        let err = reject_public_default_paths("benchmark_root = '/home/jake/Projects/x'")
            .expect_err("local paths must be rejected");
        assert!(err.to_string().contains("/home/jake"));
    }
}
