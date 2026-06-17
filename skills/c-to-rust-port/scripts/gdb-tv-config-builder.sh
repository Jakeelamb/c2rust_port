#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat >&2 <<'EOF'
usage: gdb-tv-config-builder.sh <source-dir> <rust-dir> [out-dir]

Required env:
  ACTIVE_FUNCTION=function_name
  ACTIVE_FIXTURE='small fixture arg or command note'
  SOURCE_BIN=/path/to/source-debug-binary
  RUST_BIN=/path/to/rust-debug-binary
EOF
}

if [[ $# -lt 2 || $# -gt 3 ]]; then
  usage
  exit 2
fi

source_dir=$1
rust_dir=$2
out_dir=${3:-"$rust_dir/.port-work/gdb-tv-config"}
active_fn=${ACTIVE_FUNCTION:-}
active_fixture=${ACTIVE_FIXTURE:-}
source_bin=${SOURCE_BIN:-}
rust_bin=${RUST_BIN:-}

script_dir=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
mkdir -p "$out_dir"

summary="$out_dir/SUMMARY.md"
config="$out_dir/gdb-tv.config.toml"
run_script="$out_dir/run-gdb-tv.sh"
readiness="$out_dir/readiness.md"
binary_info="$out_dir/binary-info.txt"

missing=()
[[ -n "$active_fn" ]] || missing+=("ACTIVE_FUNCTION")
[[ -n "$active_fixture" ]] || missing+=("ACTIVE_FIXTURE")
[[ -n "$source_bin" ]] || missing+=("SOURCE_BIN")
[[ -n "$rust_bin" ]] || missing+=("RUST_BIN")

if [[ -n "$source_bin" && ! -x "$source_bin" ]]; then
  missing+=("executable_SOURCE_BIN")
fi
if [[ -n "$rust_bin" && ! -x "$rust_bin" ]]; then
  missing+=("executable_RUST_BIN")
fi

: > "$binary_info"
check_debug_binary() {
  local label=$1
  local path=$2
  [[ -n "$path" && -e "$path" ]] || return 0
  if command -v file >/dev/null 2>&1; then
    info=$(file "$path" 2>/dev/null || true)
    printf '%s: %s\n' "$label" "$info" >> "$binary_info"
    if [[ "$info" == *"stripped"* && "$info" != *"not stripped"* ]]; then
      missing+=("debug_${label}")
    fi
  fi
}

check_debug_binary SOURCE_BIN "$source_bin"
check_debug_binary RUST_BIN "$rust_bin"

c_sync=${active_fn:-c_function}
rust_sync=${active_fn:-rust_function}
rust_name_map="^(?:.+::)?${rust_sync}$"

cat > "$config" <<EOF
# Generated gdb-tv config. Edit args/name maps after verifying symbols.
c_bin = "${source_bin:-/path/to/source-debug-binary}"
rust_bin = "${rust_bin:-/path/to/rust-debug-binary}"

c_arg = [
  "${active_fixture:-/path/to/small-fixture-or-arg}"
]
rust_arg = [
  "${active_fixture:-/path/to/small-fixture-or-arg}"
]

sync = [
  "${c_sync}=${rust_sync}:return"
]

name_map = [
  "^${c_sync}$=${rust_name_map}"
]

skip_c_file = [
  "*/libstdc++/*",
  "*/glibc/*"
]
skip_rust_file = [
  "*/rustc/*",
  "*/.cargo/registry/*"
]

timeout = 30
max_steps = 1000
reorder_window = 0
EOF

cat > "$run_script" <<EOF
#!/usr/bin/env bash
set -euo pipefail
"$script_dir/gdb-tv-brief.sh" "$config" "$out_dir/run"
EOF
chmod 755 "$run_script"

{
  echo "# gdb-tv Readiness"
  echo
  echo "active_function=${active_fn:-missing}"
  echo "active_fixture=${active_fixture:-missing}"
  echo "source_bin=${source_bin:-missing}"
  echo "rust_bin=${rust_bin:-missing}"
  echo
  echo "Required before running:"
  echo "- Source binary built with debug info and debugger-friendly optimization."
  echo "- Rust binary built with debug info and opt-level 0."
  echo "- Single-threaded arguments or environment."
  echo "- Sync point symbols visible to GDB on both sides."
  echo "- Fixture reaches the active function quickly."
  echo
  echo "Binary info: $binary_info"
} > "$readiness"

if [[ "${#missing[@]}" -eq 0 ]]; then
  status=ready_to_run
  first_blocker=none
  next_action="run run-gdb-tv.sh; treat func/name-map failures as config blockers before patching code"
else
  status=blocked
  first_blocker="missing $(IFS=,; echo "${missing[*]}")"
  next_action="fill debug binaries, active function, and smallest single-thread fixture before running gdb-tv"
fi

{
  echo "# gdb-tv Config Builder"
  echo
  echo "generated_at=$(date -u +%Y-%m-%dT%H:%M:%SZ)"
  echo "truth_policy=this is debugger config scaffolding, not divergence evidence"
  echo "status=$status"
  echo "first_blocker=$first_blocker"
  echo "next_action=$next_action"
  echo
  echo "source=$source_dir"
  echo "rust=$rust_dir"
  echo "out=$out_dir"
  echo
  echo "## Artifacts"
  echo "- config: $config"
  echo "- run script: $run_script"
  echo "- readiness: $readiness"
  echo "- binary info: $binary_info"
} > "$summary"

echo "$summary"
[[ "$status" == ready_to_run ]] || exit 2
