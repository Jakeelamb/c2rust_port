# Role Loop

## Translator

Input:

- One source unit or recursion group.
- Relevant Rust target excerpt.
- Compact rows from `.port-work/ccc/SUMMARY.md`, `order.csv`, a translation repair packet, or direct source inspection.
- Current contract and verification evidence.
- Manifesto gate status from `PORT_CONTEXT.md` or `references/rewrite-manifesto.md`.

Rules:

- Do not run commands.
- Do not edit the shared worktree.
- Do not use git, Cargo, package managers, tests, benchmarks, or broad scans.
- Do not load raw tool reports when compact rows are sufficient.
- Preserve source structure, names, field order, enum values, integer widths, signedness, and control flow where practical.
- Preserve output formats, parse behavior, filenames, exit behavior, and unsupported-feature errors from the original surface.
- Return source evidence, one proposed diff, and assumptions.
- Mark any unimplemented original behavior as fail-loud, not silently ignored.
- For missing/stubbed functions, translate only the packeted source unit; if prerequisites are absent, name the smallest prerequisite instead of inventing behavior.

## Source-Fidelity Reviewer

Refute the draft against the source.

Check:

- Branch behavior.
- Error codes and cleanup paths.
- Macros and preprocessor-dependent behavior.
- Floating-point operation order.
- Constants and strings.
- Function pointer, callback, virtual dispatch, and table-driven behavior.
- File format and output formatting.
- Manifesto gates: credit lineage, exact emulation, pinned versions, validation gap honesty, and fail-loud scope.

Return only findings using `review-schema.md`.

## Rust Reviewer

Refute Rust correctness and maintainability.

Check:

- Ownership and aliasing.
- Pointer/nullability translation.
- Struct layout and ABI-sensitive types.
- Unsafe invariants and missing `SAFETY:` proof.
- Borrowing that forbids valid C aliasing.
- Extra allocation, copies, hashing, synchronization, or ordering changes on hot paths.
- Likely compile errors from module paths, trait bounds, or lifetimes.

Return only findings using `review-schema.md`.

## Judge

Decide reviewer findings.

Rules:

- Send confirmed blockers to apply/converge.
- Downgrade speculative findings to `RISK` with the smallest deciding evidence.
- Reject claims contradicted by source evidence or targeted parity checks.
- Do not approve behavior without evidence.
- Do not approve release/compatibility/performance claims without the manifesto fields and artifact paths.

## Apply/Converge

This is the only phase that edits shared files or runs commands.

Responsibilities:

- Apply accepted changes.
- Run `scripts/equivalence-ladder.sh <source> <rust>` when source/Rust paths exist, or the smallest relevant verification command when only one tool applies.
- If CCC reports missing/stubbed active code, run `scripts/translation-repair-plan.sh <source> <rust>`, patch one packet, and rerun CCC before tracehash/gdb-tv.
- Capture exact pass/fail output.
- Write or refresh compact artifacts under `.port-work/` before handing work to another agent.
- Treat existing repo docs, ledgers, and prior artifacts as hints until refreshed by current tools.
- If verification fails, split the report into independent fix units before launching more workers.
- Escalate equivalence tools in this order: `ccc-rs`, tracehash hash mode, tracehash deep mode, `gdb-tv`.
- Update `PORT_CONTEXT.md` only for reusable contract facts; keep transient failures in `.port-work/**/SUMMARY.md`.
