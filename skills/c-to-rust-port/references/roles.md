# Role Loop

## Translator

Input:

- One source unit or recursion group.
- Relevant Rust target excerpt.
- Compact rows from `.port-work/ccc/SUMMARY.md`, `order.csv`, or direct source inspection.
- Current contract and verification evidence.

Rules:

- Do not run commands.
- Do not edit the shared worktree.
- Do not use git, Cargo, package managers, tests, benchmarks, or broad scans.
- Do not load raw tool reports when compact rows are sufficient.
- Preserve source structure, names, field order, enum values, integer widths, signedness, and control flow where practical.
- Return source evidence, one proposed diff, and assumptions.

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

## Apply/Converge

This is the only phase that edits shared files or runs commands.

Responsibilities:

- Apply accepted changes.
- Run the smallest relevant verification command.
- Capture exact pass/fail output.
- Write or refresh compact artifacts under `.port-work/` before handing work to another agent.
- If verification fails, split the report into independent fix units before launching more workers.
- Escalate equivalence tools in this order: `ccc-rs`, `tracehash`, `gdb-tv`.
