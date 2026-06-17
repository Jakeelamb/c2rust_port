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

if ! command -v gdb-tv >/dev/null 2>&1; then
  echo "gdb-tv not found on PATH" >&2
  exit 127
fi

mkdir -p "$out_dir"

stdout_ndjson="$out_dir/stdout.ndjson"
stderr_log="$out_dir/stderr.log"
brief_jsonl="$out_dir/brief.ndjson"
summary_md="$out_dir/SUMMARY.md"

set +e
gdb-tv --config "$config" > "$stdout_ndjson" 2> "$stderr_log"
status=$?
set -e

awk '/"event":"divergence"/ || /"event":"done"/ || /"event":"sync_ok"/ || /"event":"finish_ok"/ { print; if (/"event":"divergence"/ || /"event":"done"/) exit }' "$stdout_ndjson" > "$brief_jsonl"

{
  echo "# GDB Translation Verifier Brief"
  echo
  echo "config=$config"
  echo "out=$out_dir"
  echo "exit=$status"
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

echo "$summary_md"
exit "$status"
