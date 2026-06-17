# c-to-rust-port skill

This repo is now a skill-first porting workflow. The old `c2rust-port` Rust CLI and generated evidence contract scaffolding have been removed from the active design.

Canonical local skill:

```text
skills/c-to-rust-port/SKILL.md
```

Cursor shim:

```text
.cursor/skills/c2rust-port-orchestration/SKILL.md
```

The skill coordinates C/C++ to Rust ports around source-backed evidence, bottom-up translation order, adversarial review, and targeted equivalence tools:

- `ccc-rs` / code-complexity-comparator for static shape, missing function, constants, call-graph, and bottom-up order checks.
- `tracehash` / `tracehash-compare` for paired function I/O trace parity.
- `gdb-tv` / gdb-translation-verifier-rs for single-threaded debugger-level divergence localization.
- `rewrites.bio` and `rustification` as policy context for maintained, faithful bioinformatics rewrites.

The active design follows Matt Pocock-style skill mechanics: compact model-invoked routing, one-loop completion criteria, tool-generated summaries before raw data, and branch-specific references loaded only when needed.
