#!/usr/bin/env bash
set -euo pipefail

usage() {
  echo "usage: gdb-tv-brief.sh <config.toml> [out-dir]" >&2
}

if [[ $# -lt 1 || $# -gt 2 ]]; then
  usage
  exit 2
fi

config=$1
out_dir=${2:-".port-work/gdb-tv"}

mkdir -p "$out_dir"

stdout_ndjson="$out_dir/stdout.ndjson"
stderr_log="$out_dir/stderr.log"
brief_jsonl="$out_dir/brief.ndjson"
summary_md="$out_dir/SUMMARY.md"

emit_summary() {
  {
    echo "# GDB Translation Verifier Brief"
    echo
    echo "generated_at=$(date -u +%Y-%m-%dT%H:%M:%SZ)"
    echo "truth_policy=current gdb-tv output is authoritative for debugger sync behavior; repo docs and prior runs are hints"
    echo "status=$status_label"
    echo "exit=$status"
    echo "first_blocker=$first_blocker"
    echo "next_action=$next_action"
    echo
    echo "config=$config"
    echo "out=$out_dir"
    echo
    echo "## Artifacts"
    echo "- stdout: $stdout_ndjson"
    echo "- stderr: $stderr_log"
    echo "- brief: $brief_jsonl"
    echo
    echo "## First Events"
    sed -n '1,80p' "$brief_jsonl"
    echo
    echo "## Stderr Preview"
    sed -n '1,40p' "$stderr_log"
  } > "$summary_md"
}

: > "$stdout_ndjson"
: > "$stderr_log"
: > "$brief_jsonl"

if ! command -v gdb-tv >/dev/null 2>&1; then
  echo "gdb-tv not found on PATH" > "$stderr_log"
  status=127
  status_label=blocked
  first_blocker="gdb-tv missing"
  next_action="install gdb-tv before using debugger-level equivalence"
  emit_summary
  echo "$summary_md"
  exit "$status"
fi

if [[ ! -f "$config" ]]; then
  echo "config not found: $config" > "$stderr_log"
  status=2
  status_label=blocked
  first_blocker="gdb-tv config missing"
  next_action="create a config with debug binary paths, fixture args, sync/entry points, and name maps"
  emit_summary
  echo "$summary_md"
  exit "$status"
fi

missing_config=()
grep -Eq '(^|[[:space:]])(c_bin|c-bin)[[:space:]]*=' "$config" || missing_config+=("c_bin")
grep -Eq '(^|[[:space:]])(rust_bin|rust-bin)[[:space:]]*=' "$config" || missing_config+=("rust_bin")
grep -Eq '(^|[[:space:]])(sync|entry)[[:space:]]*=' "$config" || missing_config+=("sync_or_entry")

if [[ "${#missing_config[@]}" -gt 0 ]]; then
  printf 'config missing required fields: %s\n' "${missing_config[*]}" > "$stderr_log"
  status=2
  status_label=blocked
  first_blocker="gdb-tv config incomplete"
  next_action="add missing fields, plus single-thread debug args and name maps before running gdb-tv"
  emit_summary
  echo "$summary_md"
  exit "$status"
fi

set +e
gdb-tv --config "$config" > "$stdout_ndjson" 2> "$stderr_log"
status=$?
set -e

awk '/"event":"divergence"/ || /"event":"done"/ || /"event":"sync_ok"/ || /"event":"finish_ok"/ { print; if (/"event":"divergence"/ || /"event":"done"/) exit }' "$stdout_ndjson" > "$brief_jsonl"

if [[ "$status" -eq 0 ]]; then
  status_label=pass
  first_blocker=none
  next_action="advance to output gate or next unit"
elif grep -q '"return_value_mismatch"' "$brief_jsonl"; then
  status_label=fail
  first_blocker="return value mismatch at synced function"
  next_action="patch the synced function body; rerun gdb-tv before widening scope"
elif grep -q '"func_mismatch"' "$brief_jsonl"; then
  status_label=blocked
  first_blocker="function name mismatch at sync point"
  next_action="fix gdb-tv sync/name_map before treating this as a code bug"
elif grep -q '"arg_mismatch"' "$brief_jsonl"; then
  status_label=blocked
  first_blocker="argument mismatch at sync point"
  next_action="add arg_map/watch_map for comparable values"
elif grep -qi "ptrace\\|Operation not permitted" "$stderr_log"; then
  status_label=blocked
  first_blocker="sandbox denied GDB ptrace"
  next_action="rerun gdb-tv outside sandbox/escalated; do not change code from this result"
elif [[ "$status" -eq 2 ]]; then
  status_label=blocked
  first_blocker="timeout or max_steps"
  next_action="tighten sync points, add skips, or raise limits for the same small fixture"
else
  status_label=blocked
  first_blocker="gdb-tv tool/config error"
  next_action="inspect stderr preview; fix binary paths, symbols, name maps, or debug build"
fi

emit_summary

echo "$summary_md"
exit "$status"
