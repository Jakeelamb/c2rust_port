# Tool Guide

Use compact artifacts before raw outputs. Default generated path: `.port-work/`.

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
- `missing`: block closure when source functions have no Rust counterpart or only stubs.
- `compare`: inspect highest complexity deviations before accepting a translation.
- `constants-diff`: inspect magic number, string, and float drift.
- `call-graph-diff`: inspect structural rewiring and recursion mismatch.

Do not treat a clean static comparison as behavioral parity.

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
tracehash-compare /tmp/rust.tsv /tmp/c.tsv
tracehash-compare --only suspicious_fn --first 50 /tmp/rust.tsv /tmp/c.tsv
tracehash-compare --summary-only /tmp/rust.tsv /tmp/c.tsv
```

Interpretation:

- Count differences usually mean control-flow drift before or inside that function.
- Same input hash with different output hash means local behavior drift.
- Missing inputs mean one side reached different states or skipped cases.

## gdb-tv

Use `gdb-tv` only when both sides can run under debugger-friendly conditions.

Required conditions:

- Single-threaded.
- C/C++ built with `-O0 -g`.
- Rust built with opt-level 0 and debug info.
- Similar call graph shape or explicit sync points.

Typical sync-mode invocation:

```bash
gdb-tv \
  --c-bin /path/to/reference \
  --rust-bin /path/to/port \
  --c-arg input.dat --rust-arg input.dat \
  --sync 'process_chunk=crate::process_chunk:return' \
  --name-map '^process_chunk$=^(?:.+::)?process_chunk$' \
  --timeout 30 --max-steps 1000
```

Use TOML config for non-trivial sync points, argument maps, watch expressions, and skip lists.

Do not use `gdb-tv` for optimized or multithreaded targets.

## Benchmark And Output Gates

Use output diffs, golden fixtures, and benchmarks after implementation has a plausible source-backed translation.

Rules:

- Same inputs, same thread counts, comparable compiler optimization.
- Real datasets before performance claims.
- Compare both CLI and library surfaces when the Rust port exposes a library path.
- Treat speedups as suspicious until explained; many false wins come from skipped work or unfair setup.
