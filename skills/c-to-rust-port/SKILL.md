---
name: c-to-rust-port
description: Orchestrate faithful C/C++ to Rust ports. Use for bottom-up translation, source-fidelity review, ccc-rs checks, tracehash parity, gdb-tv divergence, compact evidence loops, and apply/converge.
---

# C To Rust Port

Use this skill to drive a C/C++ to Rust port from compact evidence. The skill owns workflow, role boundaries, tool selection, review, and equivalence escalation.

## Loop

1. **Snapshot**: run `scripts/port-snapshot.sh <source> [rust]`. Read the compact output before opening broad files.
2. **Queue**: if `ccc-rs` is available, run `scripts/ccc-brief.sh <source> <rust>`. Use `order.csv`, `missing.txt`, and top static deviations to choose the unit.
3. **Packet**: load one narrow source excerpt, target excerpt, relevant compact rows, and current verification evidence. Do not load raw reports unless the compact row points there.
4. **Draft/review**: use `references/roles.md` and `references/review-schema.md`.
5. **Converge**: apply only accepted blockers, run the smallest proof command, and record the command plus result.

Completion criterion: one unit has source evidence, a Rust patch or blocker, and a named verification/parity next step.

## Rules

- Tools before guesses. If a cheap tool can answer the question, run it or ask apply/converge to run it.
- Summaries before raw data. Prefer `.port-work/**/SUMMARY.md`, TSV, and top-N output.
- Source/Rust name matches are hypotheses until backed by static, trace, debugger, or direct source evidence.
- Output diffs and benchmarks are acceptance gates, not the main discovery mechanism.
- Worker phases are no-build and no-mutation. Apply/converge owns edits and commands.
- Treat performance or memory deltas in either direction as regressions until evidence explains them.

## References

- `references/tools.md`: when choosing or interpreting `ccc-rs`, `tracehash`, `gdb-tv`, output diffs, and benchmarks.
- `references/roles.md`: when launching translator/reviewer/judge/apply phases.
- `references/review-schema.md`: when writing or judging adversarial findings.
- `references/repo-context.md`: when setting up repo-local domain language and artifact paths.
