# c2rust_port

`c2rust_port` maps C/C++ repositories and prepares bounded Rust porting work. V1 is a planner and evidence collector, not an autonomous editor.

Binary name: `c2rust-port`.

## Workflow

```bash
c2rust-port init /path/to/source-c-repo
c2rust-port inspect /path/to/source-c-repo
c2rust-port bench prepare /path/to/source-c-repo
c2rust-port bench run-source /path/to/source-c-repo
c2rust-port packets /path/to/source-c-repo /path/to/source-name-rs
```

`init` is dry by default. Pass `--apply` to create the target scaffold.

## Examples

Separate source and target, Bowtie-style:

```bash
c2rust-port init /repos/bowtie2 --apply
c2rust-port inspect /repos/bowtie2
c2rust-port packets /repos/bowtie2 /repos/bowtie2-rs
```

Vendored source, SPAdes-style:

```bash
c2rust-port init --source spades-rs/reference/upstream/SPAdes-4.2.0 --target spades-rs --dry-run
```

## Tools

Required for useful inspection: `clang`, `clang++`, `clang-tidy`, `clang-query`, `clangd`, `bear`, `ctags`, `cflow`, `joern`, `codeql`, `perf`, `valgrind`, `gprof`, `gcov`, `rr`, `cargo`, `cargo flamegraph`, `cargo-llvm-cov`, and `seqtk`.

On Arch, install missing `seqtk` with:

```bash
yay -S seqtk
```

`inspect` attempts:

```bash
repo-system-map rewrite-prep <source> --source auto --target rust
repo-system-map semantic-export <source> --tool clang --emit all
```

Failures are recorded as diagnostics instead of stopping the whole run.

## Local Config

Public defaults do not include machine-local paths. Put local paths in ignored `.c2rust-port.local.toml`:

```toml
benchmark_root = "/path/to/port-bench-data"
biological_data_root = "/path/to/biological data"
repo_system_map = "repo-system-map"
```

## Outputs

Inspection writes:

- `.c2rust-port/inspect/tool-audit.json`
- `.c2rust-port/inspect/build-system.json`
- `.c2rust-port/inspect/source-inventory.json`
- `.c2rust-port/inspect/entrypoints.json`
- `.c2rust-port/inspect/diagnostic-runs.jsonl`

Benchmark preparation writes tiny, smoke, medium, and large manifests. `full` is intentionally manual.

Packets write `.c-to-rust-port/units/*/TASK.md`, `.c-to-rust-port/prompt_profiles/*.toml`, and `.c-to-rust-port/packet_outcomes.jsonl`.

## Safety Model

Mapper commands may run diagnostics and instrumentation. Translator packets are draft-only and forbid git, Cargo, builds, benchmarks, package managers, broad scans, shared-worktree edits, and mutation. Apply/converge packets are the only phase allowed to edit and verify.
