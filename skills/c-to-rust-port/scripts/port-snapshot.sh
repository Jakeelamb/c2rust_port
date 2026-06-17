#!/usr/bin/env bash
set -euo pipefail

usage() {
  echo "usage: port-snapshot.sh <source-dir> [rust-dir]" >&2
}

if [[ $# -lt 1 || $# -gt 2 ]]; then
  usage
  exit 2
fi

source_dir=$1
rust_dir=${2:-}

if [[ ! -d "$source_dir" ]]; then
  echo "source directory not found: $source_dir" >&2
  exit 2
fi

if [[ -n "$rust_dir" && ! -d "$rust_dir" ]]; then
  echo "rust directory not found: $rust_dir" >&2
  exit 2
fi

count_files() {
  local root=$1
  shift
  find "$root" \
    \( -path '*/.git' -o -path '*/target' -o -path '*/.venv' -o -path '*/node_modules' -o -path '*/vendor' -o -path '*/build' -o -path '*/.port-work' \) -prune \
    -o -type f \( "$@" \) -print 2>/dev/null | wc -l | tr -d ' '
}

has_cmd() {
  command -v "$1" >/dev/null 2>&1 && echo yes || echo no
}

git_ref() {
  local root=$1
  if git -C "$root" rev-parse --is-inside-work-tree >/dev/null 2>&1; then
    git -C "$root" rev-parse --short HEAD 2>/dev/null || echo unknown
  else
    echo none
  fi
}

git_dirty_count() {
  local root=$1
  if git -C "$root" rev-parse --is-inside-work-tree >/dev/null 2>&1; then
    git -C "$root" status --short 2>/dev/null | wc -l | tr -d ' '
  else
    echo na
  fi
}

echo "# Port Snapshot"
echo
echo "generated_at=$(date -u +%Y-%m-%dT%H:%M:%SZ)"
echo "truth_policy=fresh tool output beats repo docs, ledgers, memory, and prior artifacts"
echo
echo "source=$source_dir"
if [[ -n "$rust_dir" ]]; then
  echo "rust=$rust_dir"
fi
echo
echo "## Source"
echo "c_files=$(count_files "$source_dir" -name '*.c' -o -name '*.h')"
echo "cpp_files=$(count_files "$source_dir" -name '*.cc' -o -name '*.cpp' -o -name '*.cxx' -o -name '*.hh' -o -name '*.hpp' -o -name '*.hxx')"
echo "makefile=$([[ -f "$source_dir/Makefile" || -f "$source_dir/makefile" ]] && echo yes || echo no)"
echo "cmake=$([[ -f "$source_dir/CMakeLists.txt" ]] && echo yes || echo no)"
echo "compile_commands=$([[ -f "$source_dir/compile_commands.json" ]] && echo yes || echo no)"
echo "git_head=$(git_ref "$source_dir")"
echo "git_dirty_count=$(git_dirty_count "$source_dir")"
echo
if [[ -n "$rust_dir" ]]; then
  echo "## Rust"
  echo "rust_files=$(count_files "$rust_dir" -name '*.rs')"
  echo "cargo_toml=$([[ -f "$rust_dir/Cargo.toml" ]] && echo yes || echo no)"
  echo "port_context=$([[ -f "$rust_dir/PORT_CONTEXT.md" ]] && echo yes || echo no)"
  echo "git_head=$(git_ref "$rust_dir")"
  echo "git_dirty_count=$(git_dirty_count "$rust_dir")"
  echo
fi
echo "## Tools"
for tool in ccc-rs tracehash-compare gdb-tv rg git cargo cc clang gdb; do
  echo "$tool=$(has_cmd "$tool")"
done
echo
echo "## Behavior Inputs"
echo "tracehash_rust=$([[ -n "${TRACEHASH_RUST:-}" ]] && echo "$TRACEHASH_RUST" || echo missing)"
echo "tracehash_source=$([[ -n "${TRACEHASH_SOURCE:-}" ]] && echo "$TRACEHASH_SOURCE" || echo missing)"
echo "gdb_tv_config=$([[ -n "${GDB_TV_CONFIG:-}" ]] && echo "$GDB_TV_CONFIG" || echo missing)"
echo
echo "## Prior Compact Artifacts"
echo "These are hints only. Refresh before using as proof."
found_artifact=0
for path in \
  "$source_dir/PORT_CONTEXT.md" \
  "$source_dir/.port-work/ccc/SUMMARY.md" \
  "$source_dir/.port-work/tracehash/SUMMARY.md" \
  "$source_dir/.port-work/gdb-tv/SUMMARY.md"; do
  if [[ -f "$path" ]]; then
    echo "$path"
    found_artifact=1
  fi
done
if [[ -n "$rust_dir" ]]; then
  for path in \
    "$rust_dir/PORT_CONTEXT.md" \
    "$rust_dir/.port-work/ccc/SUMMARY.md" \
    "$rust_dir/.port-work/tracehash/SUMMARY.md" \
    "$rust_dir/.port-work/gdb-tv/SUMMARY.md"; do
    if [[ -f "$path" ]]; then
      echo "$path"
      found_artifact=1
    fi
  done
fi
if [[ "$found_artifact" -eq 0 ]]; then
  echo "(none)"
fi
