---
name: c-to-rust-port
description: Orchestrate faithful C/C++ to Rust ports. Use for rewrites.bio-aligned bioinformatics rewrites, bottom-up translation, source-fidelity review, ccc-rs checks, tracehash parity, gdb-tv divergence, compact evidence loops, and apply/converge.
---

# C To Rust Port

Use this skill to drive a C/C++ to Rust rewrite from compact evidence. The skill owns manifesto adherence, workflow, role boundaries, tool selection, review, and equivalence escalation.

## Loop

1. **Contract**: read or create the minimal `PORT_CONTEXT.md` fields in `references/repo-context.md`. Check `references/rewrite-manifesto.md` before accepting scope, outputs, or validation claims.
2. **Snapshot**: run `scripts/port-snapshot.sh <source> [rust]`. Read the compact output before opening broad files.
3. **Queue**: run `scripts/ccc-brief.sh <source> <rust>` when `ccc-rs` is available. Use `order.csv`, `missing.txt`, constants/call-graph drift, and top deviations to choose one unit.
4. **Packet**: load one narrow source excerpt, target excerpt, relevant compact rows, and current verification evidence. Do not load raw reports unless the compact row points there.
5. **Draft/review**: use `references/roles.md` and `references/review-schema.md`.
6. **Converge**: apply only accepted blockers, run the smallest proof command, and record command, result, artifact path, and remaining gap.

Completion criterion: one unit has source evidence, manifesto status, a Rust patch or blocker, compact tool artifact, and a named verification/parity next step.

## Rules

- Tools before guesses. If a cheap tool can answer the question, run it or ask apply/converge to run it.
- Use the fixed ladder: `ccc-rs` for static queue and shape, `tracehash` hash mode for behavioral mismatch, `tracehash` deep mode for values, `gdb-tv` for debugger-level first divergence when conditions fit.
- Summaries before raw data. Prefer `.port-work/**/SUMMARY.md`, TSV top-N output, and first-divergence rows.
- Source/Rust name matches are hypotheses until backed by static, trace, debugger, or direct source evidence.
- Output diffs and benchmarks are acceptance gates, not the main discovery mechanism.
- Worker phases are no-build and no-mutation. Apply/converge owns edits and commands.
- Treat performance or memory deltas in either direction as regressions until evidence explains them.
- Emulate exactly unless `PORT_CONTEXT.md` says otherwise: filenames, headers, columns, ordering, parsing, exit behavior, summaries, numeric precision, and unsupported flags.
- Fail loudly for out-of-scope features. Never silently ignore an original CLI flag, format field, error path, or validation gap.

## References

- `references/rewrite-manifesto.md`: when setting scope, validation, compatibility, disclosure, and release claims.
- `references/tools.md`: when choosing or interpreting `ccc-rs`, `tracehash`, `gdb-tv`, output diffs, and benchmarks.
- `references/roles.md`: when launching translator/reviewer/judge/apply phases.
- `references/review-schema.md`: when writing or judging adversarial findings.
- `references/repo-context.md`: when setting up repo-local domain language and artifact paths.
