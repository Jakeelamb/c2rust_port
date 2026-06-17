#!/usr/bin/env bash
set -euo pipefail

usage() {
  echo "usage: ccc-brief.sh <source-dir> <rust-dir> [out-dir]" >&2
}

if [[ $# -lt 2 || $# -gt 3 ]]; then
  usage
  exit 2
fi

source_dir=$1
rust_dir=$2
out_dir=${3:-"$rust_dir/.port-work/ccc"}

if ! command -v ccc-rs >/dev/null 2>&1; then
  echo "ccc-rs not found on PATH" >&2
  exit 127
fi

mkdir -p "$out_dir"

source_json="$out_dir/source.json"
rust_json="$out_dir/rust.json"
order_csv="$out_dir/order.csv"
missing_txt="$out_dir/missing.txt"
compare_json="$out_dir/compare.json"
constants_txt="$out_dir/constants-diff.txt"
callgraph_txt="$out_dir/call-graph-diff.txt"
summary="$out_dir/SUMMARY.md"

ccc-rs analyze "$source_dir" --recurse -o "$source_json"
ccc-rs analyze "$rust_dir" -l rust --recurse -o "$rust_json"
ccc-rs order "$source_dir" --recurse -o "$order_csv"
ccc-rs missing "$rust_json" "$source_json" > "$missing_txt"
ccc-rs compare "$rust_json" "$source_json" --format json > "$compare_json"
ccc-rs constants-diff "$rust_json" "$source_json" > "$constants_txt"
ccc-rs call-graph-diff "$rust_json" "$source_json" > "$callgraph_txt"

{
  echo "# CCC Brief"
  echo
  echo "source=$source_dir"
  echo "rust=$rust_dir"
  echo "out=$out_dir"
  echo
  echo "## Artifacts"
  echo "- order: $order_csv"
  echo "- missing: $missing_txt"
  echo "- compare: $compare_json"
  echo "- constants: $constants_txt"
  echo "- callgraph: $callgraph_txt"
  echo
  echo "## Missing / Stub Preview"
  sed -n '1,40p' "$missing_txt"
  echo
  echo "## Constants Drift Preview"
  sed -n '1,40p' "$constants_txt"
  echo
  echo "## Call Graph Drift Preview"
  sed -n '1,40p' "$callgraph_txt"
} > "$summary"

echo "$summary"
