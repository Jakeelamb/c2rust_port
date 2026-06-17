#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat >&2 <<'EOF'
usage: fixture-discovery.sh <source-dir> <rust-dir> [out-dir]

Optional env:
  ACTIVE_FUNCTION=function_name
EOF
}

if [[ $# -lt 2 || $# -gt 3 ]]; then
  usage
  exit 2
fi

source_dir=$1
rust_dir=$2
out_dir=${3:-"$rust_dir/.port-work/fixtures"}
active_fn=${ACTIVE_FUNCTION:-}

mkdir -p "$out_dir"

summary="$out_dir/SUMMARY.md"
candidates="$out_dir/fixture-candidates.tsv"
test_hints="$out_dir/test-hints.txt"
command_hints="$out_dir/command-hints.md"

find_up() {
  local dir=$1
  local name=$2
  while [[ "$dir" != "/" && -n "$dir" ]]; do
    if [[ -f "$dir/$name" ]]; then
      printf '%s\n' "$dir/$name"
      return 0
    fi
    dir=$(dirname "$dir")
  done
  return 1
}

score_path() {
  local label=$1
  local path=$2
  local bytes=$3
  local lower score reason
  lower=$(printf '%s' "$path" | tr '[:upper:]' '[:lower:]')
  score=0
  reason=""

  case "$lower" in
    *test*|*tests*|*fixture*|*fixtures*|*golden*|*parity*|*example*|*examples*|*sample*|*samples*)
      score=$((score + 50))
      reason="${reason}fixture_path,"
      ;;
  esac
  case "$lower" in
    *.fastq|*.fq|*.fasta|*.fa|*.sam|*.bam|*.vcf|*.gfa)
      score=$((score + 30))
      reason="${reason}bio_input,"
      ;;
    *.tsv|*.csv|*.json|*.yaml|*.yml|*.txt)
      score=$((score + 15))
      reason="${reason}small_text,"
      ;;
  esac
  if (( bytes > 0 && bytes <= 1048576 )); then
    score=$((score + 30))
    reason="${reason}small,"
  elif (( bytes <= 10485760 )); then
    score=$((score + 10))
    reason="${reason}bounded,"
  else
    score=$((score - 25))
    reason="${reason}large,"
  fi
  if [[ -n "$active_fn" && "$lower" == *"${active_fn,,}"* ]]; then
    score=$((score + 25))
    reason="${reason}function_name,"
  fi

  printf '%s\t%s\t%s\t%s\t%s\n' "$score" "${reason%,}" "$label" "$bytes" "$path"
}

scan_root() {
  local label=$1
  local root=$2
  [[ -d "$root" ]] || return 0
  while IFS= read -r path; do
    bytes=$(stat -c %s "$path" 2>/dev/null || echo 0)
    score_path "$label" "$path" "$bytes"
  done < <(
    find "$root" \
      \( -path '*/.git' -o -path '*/.github' -o -path '*/target' -o -path '*/node_modules' -o -path '*/.port-work' \) -prune -o \
      -type f \
      \( -iname '*.fa' -o -iname '*.fasta' -o -iname '*.fq' -o -iname '*.fastq' -o -iname '*.sam' -o -iname '*.bam' -o -iname '*.vcf' -o -iname '*.gfa' -o -iname '*.tsv' -o -iname '*.csv' -o -iname '*.json' -o -iname '*.yaml' -o -iname '*.yml' -o -iname '*.txt' \) \
      -size -100M -print 2>/dev/null | head -300
  )
}

{
  echo -e "score\treason\troot\tbytes\tpath"
  scan_root source "$source_dir"
  scan_root rust "$rust_dir"
} | sort -t $'\t' -k1,1nr -k4,4n > "$candidates"

if command -v rg >/dev/null 2>&1; then
  set +e
  rg -n -i --glob '!target/**' --glob '!.git/**' --glob '!.port-work/**' \
    'fixture|golden|parity|expected|fastq|fasta|sam|small|ecoli|lambda|reads|benchmark' \
    "$rust_dir" "$source_dir" 2>/dev/null | head -120 > "$test_hints"
  set -e
else
  : > "$test_hints"
fi

rust_manifest=$(find_up "$rust_dir" Cargo.toml || true)
source_makefile=$(find_up "$source_dir" Makefile || true)
{
  echo "# Fixture Command Hints"
  echo
  echo "active_function=${active_fn:-missing}"
  echo
  echo "Use the smallest candidate from fixture-candidates.tsv that reaches the active function."
  echo
  if [[ -n "$rust_manifest" ]]; then
    echo "- Rust test surface: cd $(dirname "$rust_manifest") && cargo test --quiet"
  fi
  if [[ -n "$source_makefile" ]]; then
    echo "- Source build surface: cd $(dirname "$source_makefile") && make"
  fi
  echo "- Set ACTIVE_FIXTURE to the exact small command or fixture path before tracehash/gdb-tv scaffolding."
} > "$command_hints"

candidate_count=$(awk 'NR > 1 { n++ } END { print n + 0 }' "$candidates")
if (( candidate_count > 0 )); then
  status=ready
  first_blocker=none
  next_action="choose the highest scoring small fixture that reaches the active function"
else
  status=blocked
  first_blocker="no bounded fixture candidates found"
  next_action="create or identify one smallest input/command that reaches the active function"
fi

{
  echo "# Fixture Discovery"
  echo
  echo "generated_at=$(date -u +%Y-%m-%dT%H:%M:%SZ)"
  echo "truth_policy=fixture candidates are hints; only tool/test execution proves reachability"
  echo "status=$status"
  echo "first_blocker=$first_blocker"
  echo "next_action=$next_action"
  echo
  echo "source=$source_dir"
  echo "rust=$rust_dir"
  echo "out=$out_dir"
  echo "active_function=${active_fn:-missing}"
  echo "candidate_count=$candidate_count"
  echo
  echo "## Artifacts"
  echo "- candidates: $candidates"
  echo "- test hints: $test_hints"
  echo "- command hints: $command_hints"
  echo
  echo "## Top Candidates"
  sed -n '1,12p' "$candidates"
} > "$summary"

echo "$summary"
[[ "$status" == ready ]] || exit 2
