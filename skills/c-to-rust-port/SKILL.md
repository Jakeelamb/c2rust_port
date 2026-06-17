---
name: c-to-rust-port
description: Orchestrate faithful C/C++ to Rust ports. Use for rewrites.bio-aligned bioinformatics rewrites, bottom-up translation, source-fidelity review, ccc-rs checks, tracehash parity, gdb-tv divergence, compact evidence loops, and apply/converge.
---

# C To Rust Port

Drive C/C++ to Rust rewrites from compact, current evidence.

## Loop

1. **Contract**: create/read minimal `PORT_CONTEXT.md`; check `references/rewrite-manifesto.md` before accepting scope, validation, compatibility, or release claims.
2. **Readiness**: run `scripts/equivalence-ladder.sh <source> <rust>` when paths exist. Then read generated `.port-work/equivalence/EQUIVALENCE.md` and follow `next_action`.
3. **Queue**: use CCC order, missing/stub, constants, structs, call graph, and deviation summaries to choose one bottom-up unit.
4. **Repair**: when CCC reports missing/stubbed source functions, run `scripts/translation-repair-plan.sh <source> <rust>` and implement the generated packet before behavior tools.
5. **Behavior**: for one mapped non-stubbed unit, name the smallest fixture. Run `scripts/behavior-input-plan.sh <source> <rust>` to generate missing tracehash/gdb-tv inputs before adding probes/config. Use deep mode only after a named hash mismatch.
6. **Packet**: load one source function, one Rust function, one compact tool row, and one failing command.
7. **Converge**: apply confirmed blockers, run the smallest proof, and record command, result, artifact path, remaining gap, and next verification.

Done when one unit has source evidence, manifesto status, current CCC evidence, and either behavioral proof or a concrete missing-input blocker.

## Rules

- Fresh tool output beats README, STATUS, memory, prior `.port-work`, and old notes.
- Do not close a unit on CCC alone; CCC queues/blocks, behavior proves.
- A missing/stubbed active source unit is an implementation queue, not a stop: generate a repair packet, patch one unit, then rerun CCC.
- Zero-row tracehash, project-specific TSVs, missing GDB configs, optimized binaries, and multithreaded debugger runs are blockers.
- Broad CCC missing does not forbid unit-scoped behavior work; choose one mapped non-stubbed unit first.
- Prefer `.port-work/**/SUMMARY.md`, top-N rows, and first-divergence rows before raw reports.
- Never benchmark until output/trace/debugger parity is plausible.
- Worker phases are no-build and no-mutation. Apply/converge owns edits and commands.
- Emulate exactly unless `PORT_CONTEXT.md` says otherwise: filenames, headers, columns, ordering, parsing, exits, summaries, numeric precision, unsupported flags.

## References

- `references/tools.md`: tool order, readiness, cadence, and interpretation.
- `references/roles.md`: translator/reviewer/judge/apply boundaries.
- `references/review-schema.md`: adversarial findings.
- `references/repo-context.md`: `PORT_CONTEXT.md` and artifact paths.
