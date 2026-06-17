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
focused_txt="$out_dir/focused.txt"
summary_md="$out_dir/SUMMARY.md"

tracehash-compare --left-label rust --right-label source --summary-only "$rust_trace" "$source_trace" > "$summary_txt"

if [[ -n "$only_fn" ]]; then
  tracehash-compare --left-label rust --right-label source --only "$only_fn" --first 50 "$rust_trace" "$source_trace" > "$focused_txt"
else
  : > "$focused_txt"
fi

{
  echo "# Tracehash Brief"
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
  echo "- focused: $focused_txt"
  echo
  echo "## Summary Preview"
  sed -n '1,80p' "$summary_txt"
  if [[ -n "$only_fn" ]]; then
    echo
    echo "## Focused Preview"
    sed -n '1,80p' "$focused_txt"
  fi
} > "$summary_md"

echo "$summary_md"
