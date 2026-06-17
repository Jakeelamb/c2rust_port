#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat >&2 <<'EOF'
usage: equivalence-ladder.sh <source-dir> <rust-dir> [out-dir]

Optional env:
  TRACEHASH_RUST=/path/rust.tsv
  TRACEHASH_SOURCE=/path/source.tsv
  TRACEHASH_ONLY=function_name
  GDB_TV_CONFIG=/path/config.toml
EOF
}

if [[ $# -lt 2 || $# -gt 3 ]]; then
  usage
  exit 2
fi

source_dir=$1
rust_dir=$2
out_dir=${3:-"$rust_dir/.port-work/equivalence"}

script_dir=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)

mkdir -p "$out_dir"

summary="$out_dir/EQUIVALENCE.md"
snapshot_md="$out_dir/snapshot.md"
ccc_dir="$out_dir/ccc"
repair_dir="$out_dir/translation-repair"
tracehash_dir="$out_dir/tracehash"
gdb_dir="$out_dir/gdb-tv"

overall_status=pass
first_blocker=none
next_action="advance to the next unit or output gate"
behavior_attempted=0
behavior_pass=0
repair_status=not_needed
repair_summary=""

set_status() {
  local candidate=$1
  local blocker=$2
  local action=$3
  if [[ "$overall_status" == "pass" ]]; then
    overall_status=$candidate
    first_blocker=$blocker
    next_action=$action
  elif [[ "$overall_status" == "blocked" && "$candidate" == "fail" ]]; then
    overall_status=fail
    first_blocker=$blocker
    next_action=$action
  fi
}

"$script_dir/port-snapshot.sh" "$source_dir" "$rust_dir" > "$snapshot_md"

ccc_status=0
if command -v ccc-rs >/dev/null 2>&1; then
  set +e
  ccc_summary=$("$script_dir/ccc-brief.sh" "$source_dir" "$rust_dir" "$ccc_dir" 2> "$out_dir/ccc.err")
  ccc_status=$?
  set -e
  if [[ "$ccc_status" -ne 0 ]]; then
    set_status blocked "ccc-rs exited $ccc_status" "fix CCC invocation/tooling before choosing translation units"
  elif ! grep -q "Missing in Rust (0):" "$ccc_dir/missing.txt" || ! grep -q "Partial/stubs (0):" "$ccc_dir/missing.txt"; then
    set_status fail "missing or stubbed source functions" "read translation-repair/SUMMARY.md, implement one source-backed packet, rerun CCC, then use behavior tools"
    set +e
    repair_summary=$(CCC_DIR="$ccc_dir" "$script_dir/translation-repair-plan.sh" "$source_dir" "$rust_dir" "$repair_dir" 2> "$out_dir/translation-repair.err")
    repair_status=$?
    set -e
  fi
else
  ccc_summary=""
  ccc_status=127
  set_status blocked "ccc-rs missing" "install ccc-rs or choose a unit from direct source inspection"
fi

tracehash_status=needs_inputs
tracehash_summary=""
if [[ -n "${TRACEHASH_RUST:-}" || -n "${TRACEHASH_SOURCE:-}" ]]; then
  behavior_attempted=1
  if [[ -z "${TRACEHASH_RUST:-}" || -z "${TRACEHASH_SOURCE:-}" ]]; then
    tracehash_status=2
    set_status blocked "incomplete tracehash inputs" "set both TRACEHASH_RUST and TRACEHASH_SOURCE"
  else
    set +e
    tracehash_summary=$("$script_dir/tracehash-brief.sh" "$TRACEHASH_RUST" "$TRACEHASH_SOURCE" "$tracehash_dir" "${TRACEHASH_ONLY:-}" 2> "$out_dir/tracehash.err")
    tracehash_status=$?
    set -e
    if [[ "$tracehash_status" -eq 0 ]]; then
      behavior_pass=1
    else
      if grep -q "status=fail" "$tracehash_dir/SUMMARY.md" 2>/dev/null; then
        set_status fail "tracehash mismatch" "patch the first mismatching function or add narrower probes"
      else
        set_status blocked "tracehash tool/config failure" "read tracehash summary stderr and fix trace inputs"
      fi
    fi
  fi
fi

gdb_status=needs_config
gdb_summary=""
if [[ -n "${GDB_TV_CONFIG:-}" ]]; then
  behavior_attempted=1
  set +e
  gdb_summary=$("$script_dir/gdb-tv-brief.sh" "$GDB_TV_CONFIG" "$gdb_dir" 2> "$out_dir/gdb-tv.err")
  gdb_status=$?
  set -e
  if [[ "$gdb_status" -eq 0 ]]; then
    behavior_pass=1
  else
    if grep -q "status=fail" "$gdb_dir/SUMMARY.md" 2>/dev/null; then
      set_status fail "gdb-tv divergence" "patch the synced function or return mismatch before widening scope"
    else
      set_status blocked "gdb-tv blocked" "fix gdb-tv config/sandbox/name-map before treating result as code evidence"
    fi
  fi
fi

if [[ "$overall_status" == "pass" && "$ccc_status" -eq 0 && "$behavior_attempted" -eq 0 ]]; then
  set_status blocked "behavior proof inputs missing" "choose one smallest fixture, then produce TRACEHASH_RUST/TRACEHASH_SOURCE or GDB_TV_CONFIG for the active unit"
elif [[ "$overall_status" == "pass" && "$ccc_status" -eq 0 && "$behavior_attempted" -eq 1 && "$behavior_pass" -eq 0 ]]; then
  set_status blocked "no behavioral proof passed" "fix the tracehash/gdb-tv readiness blocker before closing the unit"
fi

{
  echo "# Equivalence Ladder"
  echo
  echo "generated_at=$(date -u +%Y-%m-%dT%H:%M:%SZ)"
  echo "truth_policy=current tool output beats repo docs, ledgers, memory, prior packets, and old artifacts"
  echo "status=$overall_status"
  echo "first_blocker=$first_blocker"
  echo "next_action=$next_action"
  echo
  echo "source=$source_dir"
  echo "rust=$rust_dir"
  echo "out=$out_dir"
  echo
  echo "## Tool Status"
  echo "- snapshot: pass ($snapshot_md)"
  echo "- ccc: $ccc_status ${ccc_summary:+($ccc_summary)}"
  echo "- translation-repair: $repair_status ${repair_summary:+($repair_summary)}"
  echo "- tracehash: $tracehash_status ${tracehash_summary:+($tracehash_summary)}"
  echo "- gdb-tv: $gdb_status ${gdb_summary:+($gdb_summary)}"
  echo "- behavior_attempted: $behavior_attempted"
  echo "- behavior_pass: $behavior_pass"
  echo
  echo "## Read First"
  echo "1. $snapshot_md"
  [[ -f "$ccc_dir/SUMMARY.md" ]] && echo "2. $ccc_dir/SUMMARY.md"
  [[ -f "$repair_dir/SUMMARY.md" ]] && echo "3. $repair_dir/SUMMARY.md"
  [[ -f "$tracehash_dir/SUMMARY.md" ]] && echo "4. $tracehash_dir/SUMMARY.md"
  [[ -f "$gdb_dir/SUMMARY.md" ]] && echo "5. $gdb_dir/SUMMARY.md"
} > "$summary"

echo "$summary"
case "$overall_status" in
  pass) exit 0 ;;
  fail) exit 1 ;;
  blocked) exit 2 ;;
  *) exit 3 ;;
esac
