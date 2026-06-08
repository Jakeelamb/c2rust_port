# c2rust_port

`c2rust_port` maps one C/C++ porting repo, captures tracing evidence, prepares benchmark manifests, builds an exhaustive knowledge base, and writes bounded Rust porting packets. V1 is a deterministic planner and evidence collector, not an autonomous editor.

Binary name: `c2rust-port`.

## Command

```bash
c2rust-port /path/to/repo
```

There are no optional CLI arguments.

## How It Works

The input repo is interpreted in one of two layouts:

- C/C++ source repo: `/repos/bowtie2` maps as source `/repos/bowtie2` and target `/repos/bowtie2-rs`.
- Rust repo with vendored source: `/repos/spades-rs` maps as target `/repos/spades-rs` and source `/repos/spades-rs/reference/upstream/<first-source-dir>`.

Each run does the same phases:

1. Resolve source and target paths.
2. Create the target scaffold if it is missing.
3. Inspect the source repo.
4. Prepare tiny, smoke, medium, and large benchmark manifests.
5. Run source-build probe evidence.
6. Generate the knowledge base, raw evidence, normalized facts, repo map, and consolidation bundle.
7. Generate bounded translator packets in the target repo.

## Outputs

Source repo outputs:

- `.c2rust-port/inspect/tool-audit.json`
- `.c2rust-port/inspect/build-system.json`
- `.c2rust-port/inspect/source-inventory.json`
- `.c2rust-port/inspect/entrypoints.json`
- `.c2rust-port/inspect/diagnostic-runs.jsonl`
- `.c2rust-port/bench/manifests/*.json`
- `.c2rust-port/bench/runs/*.jsonl`
- `.c2rust-port/knowledge/knowledge-strategy.json`
- `.c2rust-port/knowledge/KNOWLEDGE.md`
- `.c2rust-port/knowledge/capability-matrix.json`
- `.c2rust-port/knowledge/capability-matrix.md`
- `.c2rust-port/knowledge/evidence.db`
- `.c2rust-port/knowledge/EVIDENCE_QUERIES.md`
- `.c2rust-port/knowledge/repo-map.json`
- `.c2rust-port/knowledge/repo-map.md`
- `.c2rust-port/knowledge/raw/evidence-runs.jsonl`
- `.c2rust-port/knowledge/raw/**`
- `.c2rust-port/knowledge/facts/*.jsonl`
- `.c2rust-port/knowledge/bundles/full-picture.md`

Target repo outputs:

- `Cargo.toml`, `src/lib.rs`, `README.md`, `PORTING.md`, `GOAL.md` when missing
- `.c-to-rust-port/STATUS.md`
- `.c-to-rust-port/SOURCE_REPO_MAP.md`
- `.c-to-rust-port/RUST_MIRROR_PLAN.md`
- `.c-to-rust-port/EVIDENCE_QUERIES.md`
- `.c-to-rust-port/CITATION_VALIDATION.json`
- `.c-to-rust-port/CITATION_VALIDATION.md`
- `.c-to-rust-port/agents/*.md`
- `.c-to-rust-port/units/*/TASK.md`
- `.c-to-rust-port/prompt_profiles/*.toml`
- `.c-to-rust-port/packet_outcomes.jsonl`

## Knowledge Model

The intended model is exhaustive upfront evidence collection:

1. Run every installed relevant mapper, tracer, build-capture, benchmark, and Rust analysis tool that can execute safely.
2. Preserve raw outputs under `.c2rust-port/knowledge/raw/<stage>/`.
3. Normalize outputs into `.c2rust-port/knowledge/facts/*.jsonl`.
4. Dedupe facts by stable keys while retaining provenance for every source tool.
5. Write `.c2rust-port/knowledge/capability-matrix.json` so agents know which tools are usable, blocked, available but unused, or missing.
6. Index all normalized rows into `.c2rust-port/knowledge/evidence.db` so agents can query source structure before editing.
7. Generate `.c2rust-port/knowledge/bundles/full-picture.md` as the reusable development map.
8. Use `repomix` as a final bundling layer when installed, alongside normalized facts and summaries.

The fact tables are:

- `tool_runs`
- `capabilities`
- `files`
- `build_units`
- `symbols`
- `types`
- `call_edges`
- `dataflow_edges`
- `feature_tags`
- `equivalence_edges`
- `equivalence_diffs`
- `diagnostics`
- `semantic_graphs`
- `runtime_events`
- `profiles`
- `coverage`
- `benchmarks`
- `rust_workspace`
- `repo_map`

Current normalizers populate these tables from repo walk, compile database or `make -n`, `bear` compile database output, `ctags`, `cflow`, `clang-query` AST output, Doxygen XML, CodeQL database artifacts, CodeQL SARIF when a local C/C++ query pack is installed, Joern CPG artifacts, compiler/linter output, benchmark manifests/runs, cargo metadata/check output, Rust source scans, and the generated repo map. Tracing-aware normalizers are already wired: `strace`/`ltrace`/debugger output normalizes to `runtime_events`, profiler output normalizes to `profiles`, and coverage output normalizes to `coverage` when those raw evidence runners are enabled.

The SQLite evidence index keeps the JSONL rows queryable without replacing them. Useful tables include `facts`, `capabilities`, `tool_runs`, `files`, `symbols`, `functions`, `types`, `call_edges`, `dataflow_edges`, `feature_tags`, `equivalence_edges`, and `equivalence_diffs`. Query recipes are generated in `.c2rust-port/knowledge/EVIDENCE_QUERIES.md` and copied into target packet context.

The source repo map records:

- Process flow: entrypoints and functions with source locations.
- Data flow: include edges and call edges with evidence labels.
- Rust mirror plan: initial Rust module paths that mirror source ownership/process boundaries before refactoring.

## Tool Audit

The audit records `name`, `category`, `purpose`, `installed`, and `path`. Useful tools are grouped across both sides of the port:

- Repo bundling: `repomix`
- C/C++ mapping: `clang`, `clang++`, `clang-tidy`, `clang-query`, `clangd`, `ctags`, `cflow`, `cscope`, `doxygen`, `joern`, `joern-parse`, `codeql`
- C/C++ build capture: `bear`, `intercept-build`, `compiledb`, `cmake`, `make`, `ninja`, `meson`, `pkg-config`
- C/C++ tracing: `llvm-cov`, `llvm-profdata`, `gprof`, `gcov`, `lcov`
- Runtime tracing: `strace`, `ltrace`, `perf`, `valgrind`, `callgrind_annotate`, `rr`, `gdb`, `lldb`, `bpftrace`, `hyperfine`, `time`, `heaptrack`
- Rust mapping: `cargo`, `rustc`, `rustdoc`, `rustfmt`, `clippy-driver`, `rust-analyzer`, `cargo-metadata`, `cargo-expand`, `cargo-modules`, `cargo-udeps`, `cargo-deny`
- Rust tracing: `cargo-nextest`, `cargo-llvm-cov`, `cargo-flamegraph`, `cargo-profiler`, `cargo-bloat`, `cargo-asm`, `cargo-instruments`
- Benchmark corpus: `seqtk`, `samtools`

On Arch, install missing `seqtk` with:

```bash
yay -S seqtk
```

## Local Config

Public defaults do not include machine-local paths. Put local paths in ignored `.c2rust-port.local.toml` if future versions need them:

```toml
benchmark_root = "/path/to/port-bench-data"
biological_data_root = "/path/to/biological data"
```

## Safety Model

Mapper phases may run diagnostics and instrumentation. Translator packets are draft-only and forbid git, Cargo, builds, benchmarks, package managers, broad scans, shared-worktree edits, and mutation. Translator drafts must cite source fact rows from `facts/*.jsonl` or `evidence.db`; uncited behavior claims are rejected before apply/converge. Generated seed citations are validated into `.c-to-rust-port/CITATION_VALIDATION.*`, and packet generation fails if any generated seed citation cannot be resolved. Failed deep-analysis tools such as Doxygen or CodeQL are surfaced as source-map blockers in generated packets. Apply/converge packets are the only phase allowed to edit and verify.
