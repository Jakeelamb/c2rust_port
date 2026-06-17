# Tool Guide

Use compact artifacts before raw outputs. Default generated path: `.port-work/`. Never open raw reports unless a compact summary identifies the function, sync point, or first mismatch to inspect.

## Decision Ladder

1. **Snapshot/readiness**: learn tool availability, source/Rust roots, fixture paths, debug binaries, prior compact artifacts, and dirty counts.
2. **CCC upfront**: pick bottom-up work, find missing/stubbed functions, static shape drift, constants drift, struct drift, call-graph drift, and floating-point operator drift.
3. **Translation repair**: when the active unit is missing/stubbed, run `translation-repair-plan.sh` and patch exactly one source-backed function packet.
4. **Fixture contract**: run `fixture-discovery.sh` or use behavior-input artifacts to choose the smallest reproducible input and active function family.
5. **Behavior input plan**: run `behavior-input-plan.sh` for the chosen mapped non-stubbed unit; it emits fixture, tracehash, and gdb-tv scaffolds before edits.
6. **Tracehash hash mode**: after both sides have matching code and paired probes, compare `function + input_hash -> output_hash`.
7. **Tracehash deep mode**: only after hash mode localizes a mismatch and scalar/struct values are needed.
8. **gdb-tv**: when instrumentation is too invasive/ambiguous, or when debugger-level first divergence is cheaper and both binaries are single-threaded, debug, and similarly shaped.
9. **Output gates**: after local parity is plausible, prove user-visible compatibility.
10. **Benchmarks**: last, after trace/debugger/output parity is plausible.

## Cadence

- Every session: snapshot + CCC.
- Every missing/stubbed active unit: one translation repair packet, one source function, one Rust target, one rerun of CCC.
- Every behavior unit: one CCC row, one source/Rust pair, one fixture-discovery summary, one smallest fixture.
- When tracehash/gdb-tv inputs are missing: produce a behavior input plan, not a fake run.
- After each patch: targeted CCC if structure changed; targeted tracehash when probes exist.
- Escalate to `gdb-tv` only for hard divergence, invasive instrumentation, or unclear tracehash results.
- Periodically/release: broader output matrix and benchmarks.

## Readiness Gate

Classify each tool before using its result:

| Tool | Ready when | Blocker if missing |
| --- | --- | --- |
| `ccc-rs` | source dir and Rust dir exist | no queue; inspect roots or install tool |
| tracehash | both traces are tracehash-format and have nonzero comparable rows | create paired probes for the active function/fixture |
| `gdb-tv` | debug single-thread binaries, args, sync/entry, name maps | create config/builds; do not infer code bugs |

Zero-row tracehash output is `blocked`, not `pass`. Project-specific TSVs such as metrics, output matrices, or custom parity traces are not tracehash evidence unless `tracehash-compare` reports nonzero comparable rows.

Do not close a unit with CCC alone. CCC can queue and block, but behavioral proof requires tracehash/gdb-tv/output evidence or a concrete missing-input blocker. Broad CCC missing should select work; it should not prevent behavior work for a chosen mapped non-stubbed unit.

Preferred loop command:

```bash
skills/c-to-rust-port/scripts/equivalence-ladder.sh <source-dir> <rust-dir>
CCC_DIR=.port-work/equivalence/ccc \
  skills/c-to-rust-port/scripts/translation-repair-plan.sh <source-dir> <rust-dir>
ACTIVE_FUNCTION=fn CCC_DIR=.port-work/equivalence/ccc \
  skills/c-to-rust-port/scripts/translation-repair-plan.sh <source-dir> <rust-dir>
ACTIVE_FUNCTION=fn ACTIVE_FIXTURE='fixture command' \
  skills/c-to-rust-port/scripts/behavior-input-plan.sh <source-dir> <rust-dir>
ACTIVE_FUNCTION=fn \
  skills/c-to-rust-port/scripts/fixture-discovery.sh <source-dir> <rust-dir>
ACTIVE_FUNCTION=fn ACTIVE_FIXTURE='fixture command' \
  skills/c-to-rust-port/scripts/tracehash-scaffold.sh <source-dir> <rust-dir>
ACTIVE_FUNCTION=fn ACTIVE_FIXTURE='fixture arg' SOURCE_BIN=/path/c RUST_BIN=/path/rust \
  skills/c-to-rust-port/scripts/gdb-tv-config-builder.sh <source-dir> <rust-dir>
TRACEHASH_RUST=rust.tsv TRACEHASH_SOURCE=c.tsv TRACEHASH_ONLY=fn \
  skills/c-to-rust-port/scripts/equivalence-ladder.sh <source-dir> <rust-dir>
GDB_TV_CONFIG=.port-work/gdb-tv/config.toml \
  skills/c-to-rust-port/scripts/equivalence-ladder.sh <source-dir> <rust-dir>
```

Run the ladder, then read generated `.port-work/equivalence/EQUIVALENCE.md` first. It gives `status`, `first_blocker`, and `next_action`. When CCC finds missing/stubbed code, the ladder also emits `.port-work/equivalence/translation-repair/SUMMARY.md`.

## Truth Discipline

Truth hierarchy for port work:

1. Current source tree plus current tool output from this run.
2. Current `.port-work/equivalence/EQUIVALENCE.md` and referenced compact summaries.
3. Freshly regenerated CCC/tracehash/gdb-tv artifacts.
4. Repo docs, `STATUS.md`, `PORT_CONTEXT.md`, old `.port-work`, memory, and chat history.

Only levels 1-3 can prove parity, missing work, performance, or correctness. Level 4 may choose where to look, but must not close a unit or justify a patch without a current tool artifact.

Rules:

- Refresh stale artifacts before using them as proof. If unsure whether stale, rerun the smallest tool.
- Do not accept a repo ledger saying “green” when current output, tracehash, gdb-tv, or tests say otherwise.
- Do not accept a repo ledger saying “blocked” when current tools can cheaply recheck it.
- Write new facts back only after current tools produce a compact artifact path.
- If generated packets disagree with current source/tool output, trust source/tool output and regenerate packets.

## Failure To Action

| Signal | Next action |
| --- | --- |
| `ccc missing` or stubs | Run `translation-repair-plan.sh`; implement/map one source-backed packet before tracehash or gdb-tv. |
| broad `ccc missing` but one mapped unit exists | Pick that active unit and run `behavior-input-plan.sh`. |
| repair packet says dependency missing | Run `translation-repair-plan.sh` for the smallest prerequisite function first. |
| no obvious fixture | Run `fixture-discovery.sh`; choose the smallest candidate that reaches the active function. |
| `ccc constants-diff` | Inspect source constants/strings before changing behavior. |
| `ccc binary_operators` drift | Suspect numeric order, dropped terms, or changed branch conditions. |
| `tracehash zero comparable rows` | Build real paired tracehash probes; do not compare arbitrary TSVs. |
| tracehash probes missing | Run `tracehash-scaffold.sh`; patch paired probes only for the active function/fixture. |
| `tracehash count differences` | Add/check branch or control-flow probes before downstream values. |
| `tracehash same input, different output` | Patch only that function; deep mode only if values are still needed. |
| `gdb-tv missing config` | Create debug binary paths, fixture args, sync/entry, and name maps. |
| `gdb-tv` inputs missing | Run `gdb-tv-config-builder.sh`; do not invoke debugger until status is `ready_to_run`. |
| `gdb-tv func_mismatch` | Fix sync point/name_map; not code evidence yet. |
| `gdb-tv arg_mismatch` | Add `arg_map`/`watch_map`; compare only semantic equivalents. |
| `gdb-tv return_value_mismatch` | Patch the synced function body, then rerun the same config. |
| `gdb-tv ptrace denied` | Rerun outside sandbox/escalated; do not edit code from that result. |
| Output diff after local parity | Compare CLI/file-format surface, not internal functions. |

## Snapshot

Run first:

```bash
skills/c-to-rust-port/scripts/port-snapshot.sh <source-dir> [rust-dir]
```

Use it to decide:

- whether `ccc-rs`, `tracehash-compare`, and `gdb-tv` are available;
- whether source build metadata exists;
- whether a prior compact artifact can be reused.

## ccc-rs

Use `ccc-rs` first. It is static, cheap, and good for queueing and triage.

Preferred compact command:

```bash
skills/c-to-rust-port/scripts/ccc-brief.sh <source-dir> <rust-dir>
```

It writes `.port-work/ccc/SUMMARY.md` plus raw JSON/TXT artifacts. Read the summary first; open raw reports only for cited rows.

Manual commands:

```bash
ccc-rs analyze path/to/c_src --recurse -o c.json
ccc-rs analyze path/to/rust_src -l rust --recurse -o rust.json
ccc-rs order path/to/c_src --recurse -o order.csv
ccc-rs compare rust.json c.json --format json
ccc-rs missing rust.json c.json
ccc-rs constants-diff rust.json c.json
ccc-rs call-graph-diff rust.json c.json
```

Use results this way:

- `order`: choose bottom-up translation units.
- `missing`: block closure when source functions have no Rust counterpart or only stubs; generate a repair packet before behavior tools.
- `compare`: inspect highest complexity deviations before accepting a translation.
- `constants-diff`: inspect magic number, string, and float drift.
- `call-graph-diff`: inspect structural rewiring and recursion mismatch.
- `compare-structs` / `missing-structs`: inspect layout-sensitive structs, records, and data models.
- `binary_operators` drift: treat arithmetic operator changes as high-risk for floating-point rounding and precision.

Do not treat a clean static comparison as behavioral parity.

### Translation Repair Packets

Use this when CCC reports a missing/stubbed active unit:

```bash
skills/c-to-rust-port/scripts/translation-repair-plan.sh <source-dir> <rust-dir>
ACTIVE_FUNCTION=fn REPAIR_KIND=stub \
  skills/c-to-rust-port/scripts/translation-repair-plan.sh <source-dir> <rust-dir>
```

Read only `SUMMARY.md`, `IMPLEMENTATION_PACKET.md`, candidate TSVs, the selected `source-function.json`, the source snippet, and the nearest Rust candidate. The packet is a work order, not proof. Implement one function, rerun CCC, then continue to behavior input planning only after the active unit is no longer missing/stubbed.

`equivalence-ladder.sh` creates this packet automatically under `.port-work/equivalence/translation-repair/` when CCC blocks on missing/stubbed functions.

## Fixtures

Use `fixture-discovery.sh` before inventing fixtures. It ranks bounded data files and test/parity hints and writes `fixture-candidates.tsv`, `test-hints.txt`, and `command-hints.md`. Candidate rows are hints; only executing the fixture proves it reaches the active function.

## tracehash

Use `tracehash` after code exists and a specific function family diverges.

Principles:

- Give paired probes the same function name.
- Hash canonical data only: explicit lengths, little-endian integers, raw float bits for bitwise parity, quantized floats only as an auxiliary signal.
- Include every external input that affects output: sequence bytes, model identity, RNG state, thresholds, mode flags, window coordinates.
- Start coarse, then move inward around the first mismatching function.
- Rebuild C without tracehash before timing.

Typical comparison:

```bash
ACTIVE_FUNCTION=fn ACTIVE_FIXTURE='fixture command' \
  skills/c-to-rust-port/scripts/tracehash-scaffold.sh <source-dir> <rust-dir>
skills/c-to-rust-port/scripts/tracehash-brief.sh /tmp/rust.tsv /tmp/c.tsv
tracehash-compare /tmp/rust.tsv /tmp/c.tsv
tracehash-compare --only suspicious_fn --first 50 /tmp/rust.tsv /tmp/c.tsv
tracehash-compare --summary-only /tmp/rust.tsv /tmp/c.tsv
```

Interpretation:

- Count differences usually mean control-flow drift before or inside that function.
- Same input hash with different output hash means local behavior drift.
- Missing inputs mean one side reached different states or skipped cases.

`tracehash-brief.sh` writes `SUMMARY.md` even when mismatches make `tracehash-compare` exit nonzero. Use its `status`, `first_blocker`, and `next_action` before opening traces.

Escalate to deep mode only when the hash summary has named the first suspicious function and values are required. Keep `TRACEHASH_DEEP_MODE` narrow (`first:N`, `TRACEHASH_DEEP_ONLY`) unless the fixture is tiny.

## gdb-tv

Use `gdb-tv` only when both sides can run under debugger-friendly conditions.

Required conditions:

- Single-threaded.
- C/C++ built with `-O0 -g`.
- Rust built with opt-level 0 and debug info.
- Similar call graph shape or explicit sync points.

Typical sync-mode invocation:

```bash
skills/c-to-rust-port/scripts/gdb-tv-brief.sh config.toml
gdb-tv \
  --c-bin /path/to/reference \
  --rust-bin /path/to/port \
  --c-arg input.dat --rust-arg input.dat \
  --sync 'process_chunk=crate::process_chunk:return' \
  --name-map '^process_chunk$=^(?:.+::)?process_chunk$' \
  --timeout 30 --max-steps 1000
```

Use TOML config for non-trivial sync points, argument maps, watch expressions, and skip lists.
Use `gdb-tv-config-builder.sh` to emit a config and readiness summary before invoking `gdb-tv-brief.sh`.

Do not use `gdb-tv` for optimized or multithreaded targets. Prefer sync-point mode before trace mode; trace mode needs skip lists or it will waste time in runtime/library frames.
Rust frames often include module prefixes such as `crate::module::func`; map C leaf names with anchored regexes such as `^func$=^(?:.+::)?func$`.

## Benchmark And Output Gates

Use output diffs, golden fixtures, and benchmarks after implementation has a plausible source-backed translation.

Rules:

- Same inputs, same thread counts, comparable compiler optimization.
- Real datasets before performance claims.
- Compare both CLI and library surfaces when the Rust port exposes a library path.
- Treat speedups as suspicious until explained; many false wins come from skipped work or unfair setup.
- Every equivalence claim names original version, Rust commit/version, input data, command lines, hardware, and artifact path.
