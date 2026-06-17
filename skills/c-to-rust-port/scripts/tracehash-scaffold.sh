#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat >&2 <<'EOF'
usage: tracehash-scaffold.sh <source-dir> <rust-dir> [out-dir]

Required env:
  ACTIVE_FUNCTION=function_name

Optional env:
  ACTIVE_FIXTURE='small fixture command or path'
  SOURCE_CMD='command that writes source trace when TRACEHASH_OUT is set'
  RUST_CMD='command that writes rust trace when TRACEHASH_OUT is set'
EOF
}

if [[ $# -lt 2 || $# -gt 3 ]]; then
  usage
  exit 2
fi

source_dir=$1
rust_dir=$2
out_dir=${3:-"$rust_dir/.port-work/tracehash-scaffold"}
active_fn=${ACTIVE_FUNCTION:-}
active_fixture=${ACTIVE_FIXTURE:-}
source_cmd=${SOURCE_CMD:-}
rust_cmd=${RUST_CMD:-}

mkdir -p "$out_dir"

safe_fn=${active_fn:-function}
safe_fn=$(printf '%s' "$safe_fn" | tr -c 'A-Za-z0-9_' '_')
source_trace="/tmp/tracehash-source-${safe_fn}.tsv"
rust_trace="/tmp/tracehash-rust-${safe_fn}.tsv"

summary="$out_dir/SUMMARY.md"
plan="$out_dir/TRACEHASH_PROBE_PLAN.md"
source_template="$out_dir/source-tracehash-probe-template.c"
rust_template="$out_dir/rust-tracehash-probe-template.rs"
run_script="$out_dir/run-tracehash-compare.sh"

missing=()
[[ -n "$active_fn" ]] || missing+=("ACTIVE_FUNCTION")
[[ -n "$active_fixture" ]] || missing+=("ACTIVE_FIXTURE")

cat > "$source_template" <<'EOF'
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>

static uint64_t tracehash_fnv1a(const void *data, size_t len) {
    const unsigned char *p = (const unsigned char *)data;
    uint64_t h = 1469598103934665603ULL;
    for (size_t i = 0; i < len; i++) {
        h ^= (uint64_t)p[i];
        h *= 1099511628211ULL;
    }
    return h;
}

static uint64_t tracehash_mix_u64(uint64_t h, uint64_t v) {
    for (int i = 0; i < 8; i++) {
        unsigned char b = (unsigned char)((v >> (i * 8)) & 0xff);
        h ^= (uint64_t)b;
        h *= 1099511628211ULL;
    }
    return h;
}

static void tracehash_emit(const char *function, uint64_t input_hash, uint64_t output_hash) {
    const char *path = getenv("TRACEHASH_OUT");
    if (path == NULL || path[0] == '\0') return;
    FILE *f = fopen(path, "a");
    if (f == NULL) return;
    fprintf(f, "%s\t%016llx\t%016llx\n",
            function,
            (unsigned long long)input_hash,
            (unsigned long long)output_hash);
    fclose(f);
}

/* At the active function boundary:
 * uint64_t in = tracehash_fnv1a(ptr, len);
 * in = tracehash_mix_u64(in, flags_or_threshold);
 * uint64_t out = tracehash_mix_u64(1469598103934665603ULL, result);
 * tracehash_emit("ACTIVE_FUNCTION", in, out);
 */
EOF

cat > "$rust_template" <<'EOF'
use std::io::Write;

fn tracehash_fnv1a(bytes: &[u8]) -> u64 {
    let mut h = 1469598103934665603u64;
    for &b in bytes {
        h ^= u64::from(b);
        h = h.wrapping_mul(1099511628211);
    }
    h
}

fn tracehash_mix_u64(mut h: u64, value: u64) -> u64 {
    for b in value.to_le_bytes() {
        h ^= u64::from(b);
        h = h.wrapping_mul(1099511628211);
    }
    h
}

fn tracehash_emit(function: &str, input_hash: u64, output_hash: u64) {
    let Some(path) = std::env::var_os("TRACEHASH_OUT") else {
        return;
    };
    let Ok(mut file) = std::fs::OpenOptions::new().create(true).append(true).open(path) else {
        return;
    };
    let _ = writeln!(file, "{function}\t{input_hash:016x}\t{output_hash:016x}");
}

// At the active function boundary:
// let mut input = tracehash_fnv1a(bytes);
// input = tracehash_mix_u64(input, flags_or_threshold as u64);
// let output = tracehash_mix_u64(1469598103934665603, result as u64);
// tracehash_emit("ACTIVE_FUNCTION", input, output);
EOF

cat > "$run_script" <<EOF
#!/usr/bin/env bash
set -euo pipefail

: "\${SOURCE_CMD:?set SOURCE_CMD to the source command for the active fixture}"
: "\${RUST_CMD:?set RUST_CMD to the Rust command for the active fixture}"

rm -f "$source_trace" "$rust_trace"
TRACEHASH_OUT="$source_trace" bash -lc "\$SOURCE_CMD"
TRACEHASH_OUT="$rust_trace" bash -lc "\$RUST_CMD"
tracehash-compare --only "${active_fn:-function}" --first 50 "$rust_trace" "$source_trace"
EOF
chmod 755 "$run_script"

cat > "$plan" <<EOF
# Tracehash Probe Scaffold

status=$([[ "${#missing[@]}" -eq 0 ]] && echo ready_to_patch || echo blocked)
active_function=${active_fn:-missing}
active_fixture=${active_fixture:-missing}
source_trace=$source_trace
rust_trace=$rust_trace

Patch contract:
- Add paired probes at the same logical function boundary.
- Use the same function label on both sides: ${active_fn:-<ACTIVE_FUNCTION>}.
- Hash all inputs that affect output before mutation.
- Hash outputs at the boundary, not downstream formatted text.
- Use explicit lengths, little-endian integers, and raw float bits for bitwise parity.
- Rebuild without probes before benchmarking.

Compare after both traces exist:

\`\`\`bash
TRACEHASH_RUST="$rust_trace" TRACEHASH_SOURCE="$source_trace" TRACEHASH_ONLY="${active_fn:-function}" \\
  skills/c-to-rust-port/scripts/equivalence-ladder.sh "$source_dir" "$rust_dir"
\`\`\`

Or run:

\`\`\`bash
SOURCE_CMD='${source_cmd:-<source command>}' RUST_CMD='${rust_cmd:-<rust command>}' "$run_script"
\`\`\`
EOF

if [[ "${#missing[@]}" -eq 0 ]]; then
  status=ready_to_patch
  first_blocker=none
  next_action="patch paired tracehash probes, run the same fixture on both sides, then run tracehash-brief/equivalence-ladder"
else
  status=blocked
  first_blocker="missing $(IFS=,; echo "${missing[*]}")"
  next_action="set ACTIVE_FUNCTION and ACTIVE_FIXTURE before patching probes"
fi

{
  echo "# Tracehash Scaffold"
  echo
  echo "generated_at=$(date -u +%Y-%m-%dT%H:%M:%SZ)"
  echo "truth_policy=this is probe scaffolding, not trace evidence"
  echo "status=$status"
  echo "first_blocker=$first_blocker"
  echo "next_action=$next_action"
  echo
  echo "source=$source_dir"
  echo "rust=$rust_dir"
  echo "out=$out_dir"
  echo "active_function=${active_fn:-missing}"
  echo "active_fixture=${active_fixture:-missing}"
  echo
  echo "## Artifacts"
  echo "- plan: $plan"
  echo "- source template: $source_template"
  echo "- rust template: $rust_template"
  echo "- run script: $run_script"
} > "$summary"

echo "$summary"
[[ "$status" == ready_to_patch ]] || exit 2
