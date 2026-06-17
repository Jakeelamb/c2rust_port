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
compare_txt="$out_dir/compare.txt"
constants_txt="$out_dir/constants-diff.txt"
callgraph_txt="$out_dir/call-graph-diff.txt"
structs_txt="$out_dir/compare-structs.txt"
missing_structs_txt="$out_dir/missing-structs.txt"
summary="$out_dir/SUMMARY.md"

ccc-rs analyze "$source_dir" --recurse -o "$source_json"
ccc-rs analyze "$rust_dir" -l rust --recurse -o "$rust_json"
ccc-rs order "$source_dir" --recurse -o "$order_csv"
ccc-rs missing "$rust_json" "$source_json" > "$missing_txt"
ccc-rs compare "$rust_json" "$source_json" --top 25 > "$compare_txt"
ccc-rs compare "$rust_json" "$source_json" --format json > "$compare_json"
ccc-rs constants-diff "$rust_json" "$source_json" > "$constants_txt"
ccc-rs call-graph-diff "$rust_json" "$source_json" > "$callgraph_txt"
ccc-rs compare-structs "$rust_json" "$source_json" --top 25 > "$structs_txt"
ccc-rs missing-structs "$rust_json" "$source_json" > "$missing_structs_txt"

if grep -q "Missing in Rust (0):" "$missing_txt" && grep -q "Partial/stubs (0):" "$missing_txt"; then
  ccc_status=pass
  ccc_next_action="choose the highest-risk static drift row, then require behavioral proof before closing"
else
  ccc_status=fail
  ccc_next_action="run translation-repair-plan.sh for one source-backed missing/stubbed function before tracehash or gdb-tv"
fi

{
  echo "# CCC Brief"
  echo
  echo "generated_at=$(date -u +%Y-%m-%dT%H:%M:%SZ)"
  echo "truth_policy=current CCC output is authoritative for static shape; repo docs and prior artifacts are hints"
  echo "status=$ccc_status"
  echo "next_action=$ccc_next_action"
  echo
  echo "source=$source_dir"
  echo "rust=$rust_dir"
  echo "out=$out_dir"
  echo
  echo "## Artifacts"
  echo "- order: $order_csv"
  echo "- missing: $missing_txt"
  echo "- compare text: $compare_txt"
  echo "- compare: $compare_json"
  echo "- constants: $constants_txt"
  echo "- callgraph: $callgraph_txt"
  echo "- structs: $structs_txt"
  echo "- missing structs: $missing_structs_txt"
  echo
  echo "## Top Static Deviations"
  sed -n '1,40p' "$compare_txt"
  echo
  echo "## Missing / Stub Preview"
  sed -n '1,40p' "$missing_txt"
  echo
  echo "## Constants Drift Preview"
  sed -n '1,40p' "$constants_txt"
  echo
  echo "## Call Graph Drift Preview"
  sed -n '1,40p' "$callgraph_txt"
  echo
  echo "## Struct Drift Preview"
  sed -n '1,40p' "$structs_txt"
  echo
  echo "## Missing Struct Preview"
  sed -n '1,40p' "$missing_structs_txt"
} > "$summary"

echo "$summary"
