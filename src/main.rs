use anyhow::Result;
use clap::Parser;

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "c2rust_port=info".into()),
        )
        .init();

    c2rust_port::run(c2rust_port::Cli::parse())
}
