#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat >&2 <<'EOF'
usage: behavior-input-plan.sh <source-dir> <rust-dir> [out-dir]

Optional env:
  ACTIVE_FUNCTION=name
  ACTIVE_FIXTURE='command or fixture path'
  SOURCE_BIN=/path/debug-c-bin
  RUST_BIN=/path/debug-rust-bin
EOF
}

if [[ $# -lt 2 || $# -gt 3 ]]; then
  usage
  exit 2
fi

source_dir=$1
rust_dir=$2
out_dir=${3:-"$rust_dir/.port-work/behavior-inputs"}
active_fn=${ACTIVE_FUNCTION:-}
active_fixture=${ACTIVE_FIXTURE:-}
source_bin=${SOURCE_BIN:-}
rust_bin=${RUST_BIN:-}

mkdir -p "$out_dir"

summary="$out_dir/SUMMARY.md"
tracehash_plan="$out_dir/tracehash-inputs.md"
gdb_template="$out_dir/gdb-tv-config.template.toml"

missing=()
[[ -n "$active_fn" ]] || missing+=("ACTIVE_FUNCTION")
[[ -n "$active_fixture" ]] || missing+=("ACTIVE_FIXTURE")

if [[ -n "$active_fn" ]]; then
  c_sync=$active_fn
  rust_sync=$active_fn
  rust_name_map="^(?:.+::)?${active_fn}$"
else
  c_sync="<c_function>"
  rust_sync="<rust_function>"
  rust_name_map="^(?:.+::)?<rust_function>$"
fi

cat > "$tracehash_plan" <<EOF
# Tracehash Input Plan

status=$([[ "${#missing[@]}" -eq 0 ]] && echo ready_to_instrument || echo needs_unit_fixture)

active_function=${active_fn:-missing}
active_fixture=${active_fixture:-missing}

Required paired probes:
- Source probe label: ${active_fn:-<same logical function name>}
- Rust probe label: ${active_fn:-<same logical function name>}
- Source trace output: /tmp/tracehash-source-${active_fn:-function}.tsv
- Rust trace output: /tmp/tracehash-rust-${active_fn:-function}.tsv

Probe contract:
- Same logical function label on both sides.
- Hash canonical inputs that affect output: sequence bytes, lengths, flags, thresholds, RNG/model state, coordinates.
- Hash canonical outputs at the function boundary.
- Use explicit lengths, little-endian integers, and raw float bits when bitwise parity matters.

Next command after probes exist:

\`\`\`bash
TRACEHASH_RUST=/tmp/tracehash-rust-${active_fn:-function}.tsv \\
TRACEHASH_SOURCE=/tmp/tracehash-source-${active_fn:-function}.tsv \\
TRACEHASH_ONLY=${active_fn:-function} \\
  skills/c-to-rust-port/scripts/equivalence-ladder.sh "$source_dir" "$rust_dir"
\`\`\`
EOF

cat > "$gdb_template" <<EOF
# gdb-tv config template
# Fill paths/args after building single-thread debug binaries.

c_bin = "${source_bin:-/path/to/source-debug-binary}"
rust_bin = "${rust_bin:-/path/to/rust-debug-binary}"

c_arg = [
  "${active_fixture:-/path/to/small-fixture}"
]
rust_arg = [
  "${active_fixture:-/path/to/small-fixture}"
]

sync = [
  "${c_sync}=${rust_sync}:return"
]

name_map = [
  "^${c_sync}$=${rust_name_map}"
]

timeout = 30
max_steps = 1000
EOF

{
  echo "# Behavior Input Plan"
  echo
  echo "generated_at=$(date -u +%Y-%m-%dT%H:%M:%SZ)"
  echo "truth_policy=this is a missing-input plan, not behavioral evidence"
  if [[ "${#missing[@]}" -eq 0 ]]; then
    echo "status=ready_to_create_inputs"
    echo "first_blocker=none"
    echo "next_action=add paired tracehash probes or fill the gdb-tv config for this active unit"
  else
    echo "status=blocked"
    echo "first_blocker=missing $(IFS=,; echo "${missing[*]}")"
    echo "next_action=choose one mapped non-stubbed unit and one smallest fixture before adding probes or debugger config"
  fi
  echo
  echo "source=$source_dir"
  echo "rust=$rust_dir"
  echo "out=$out_dir"
  echo "active_function=${active_fn:-missing}"
  echo "active_fixture=${active_fixture:-missing}"
  echo
  echo "## Artifacts"
  echo "- tracehash plan: $tracehash_plan"
  echo "- gdb-tv config template: $gdb_template"
} > "$summary"

echo "$summary"
if [[ "${#missing[@]}" -eq 0 ]]; then
  exit 0
else
  exit 2
fi
