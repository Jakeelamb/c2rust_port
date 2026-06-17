---
name: c2rust-port-orchestration
description: Cursor shim for the c-to-rust-port skill. Use when porting C/C++ to Rust, planning faithful translation, running ccc-rs static checks, tracehash parity probes, gdb-tv divergence localization, adversarial review, or apply/converge loops.
---

# c2rust-port orchestration

Canonical skill source: `skills/c-to-rust-port/SKILL.md`.

Read that file first, then load only the referenced file needed for the current task:

- `skills/c-to-rust-port/references/rewrite-manifesto.md` for rewrites.bio scope, validation, compatibility, disclosure, and release gates.
- `skills/c-to-rust-port/references/tools.md` for `ccc-rs`, `tracehash`, `tracehash-compare`, and `gdb-tv`.
- `skills/c-to-rust-port/references/roles.md` for translator/reviewer/judge/apply boundaries.
- `skills/c-to-rust-port/references/review-schema.md` for adversarial review findings.
- `skills/c-to-rust-port/references/repo-context.md` for repo-local `PORT_CONTEXT.md` and `.port-work/` conventions.

The old `c2rust-port` CLI is not the source of truth for this repo anymore.
