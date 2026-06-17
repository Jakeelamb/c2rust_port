#!/usr/bin/env bash
set -euo pipefail

usage() {
  echo "usage: tracehash-brief.sh <rust-trace.tsv> <source-trace.tsv> [out-dir] [only-function]" >&2
}

if [[ $# -lt 2 || $# -gt 4 ]]; then
  usage
  exit 2
fi

rust_trace=$1
source_trace=$2
out_dir=${3:-".port-work/tracehash"}
only_fn=${4:-}

if ! command -v tracehash-compare >/dev/null 2>&1; then
  echo "tracehash-compare not found on PATH" >&2
  exit 127
fi

mkdir -p "$out_dir"

summary_txt="$out_dir/summary.txt"
summary_err="$out_dir/summary.err"
focused_txt="$out_dir/focused.txt"
focused_err="$out_dir/focused.err"
summary_md="$out_dir/SUMMARY.md"

set +e
tracehash-compare --left-label rust --right-label source --summary-only "$rust_trace" "$source_trace" > "$summary_txt" 2> "$summary_err"
summary_status=$?

if [[ -n "$only_fn" ]]; then
  tracehash-compare --left-label rust --right-label source --only "$only_fn" --first 50 "$rust_trace" "$source_trace" > "$focused_txt" 2> "$focused_err"
  focused_status=$?
else
  : > "$focused_txt"
  : > "$focused_err"
  focused_status=0
fi
set -e

status=$summary_status
if [[ "$status" -eq 0 && "$focused_status" -ne 0 ]]; then
  status=$focused_status
fi

zero_rows=0
if grep -Eq 'traces match for 0 [^[:space:]]+ rows and 0 [^[:space:]]+ rows' "$summary_txt" "$focused_txt"; then
  zero_rows=1
  status=2
fi

if [[ "$zero_rows" -eq 1 ]]; then
  status_label=blocked
  first_blocker="zero comparable tracehash rows"
  next_action="produce paired tracehash-format probes for the active function and fixture; do not compare project-specific TSVs"
elif [[ "$status" -eq 0 ]]; then
  status_label=pass
  first_blocker=none
  next_action="advance to the next proof gate or unit"
elif grep -q "output_mismatches=[1-9]" "$summary_txt" "$focused_txt"; then
  status_label=fail
  first_blocker="same input hash with different output hash"
  next_action="inspect only the first mismatching function; patch local behavior before adding probes"
elif grep -qi "count differences\\|missing_inputs=[1-9]" "$summary_txt" "$focused_txt"; then
  status_label=fail
  first_blocker="call count or missing input drift"
  next_action="add or inspect branch/control-flow probes before trusting downstream value mismatches"
else
  status_label=fail
  first_blocker="tracehash comparator exited $status"
  next_action="read summary preview first, then focused/raw trace rows only if needed"
fi

{
  echo "# Tracehash Brief"
  echo
  echo "generated_at=$(date -u +%Y-%m-%dT%H:%M:%SZ)"
  echo "truth_policy=current tracehash output is authoritative for instrumented behavior; repo docs and prior traces are hints"
  echo "status=$status_label"
  echo "exit=$status"
  echo "first_blocker=$first_blocker"
  echo "next_action=$next_action"
  echo
  echo "rust=$rust_trace"
  echo "source=$source_trace"
  echo "out=$out_dir"
  if [[ -n "$only_fn" ]]; then
    echo "only=$only_fn"
  fi
  echo
  echo "## Artifacts"
  echo "- summary: $summary_txt"
  echo "- summary stderr: $summary_err"
  echo "- focused: $focused_txt"
  echo "- focused stderr: $focused_err"
  echo
  echo "## Summary Preview"
  sed -n '1,80p' "$summary_txt"
  if [[ -s "$summary_err" ]]; then
    echo
    echo "## Summary Stderr Preview"
    sed -n '1,40p' "$summary_err"
  fi
  if [[ -n "$only_fn" ]]; then
    echo
    echo "## Focused Preview"
    sed -n '1,80p' "$focused_txt"
    if [[ -s "$focused_err" ]]; then
      echo
      echo "## Focused Stderr Preview"
      sed -n '1,40p' "$focused_err"
    fi
  fi
} > "$summary_md"

echo "$summary_md"
exit "$status"
