#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'USAGE'
Usage: scripts/sync-c-to-rust-skill.sh [--pull] [--expect COMMIT] [--install-root DIR]

Sync the canonical c-to-rust-port skill from this c2rust_port checkout into the
active Codex skill directory.

This script intentionally never runs git inside ~/.codex/skills or
agent-scripts. Those paths are installation targets, not the source of truth.
USAGE
}

pull=0
expect=""
install_root="${CODEX_HOME:-$HOME/.codex}/skills"
skill_name="c-to-rust-port"

while [[ $# -gt 0 ]]; do
  case "$1" in
    --pull)
      pull=1
      ;;
    --expect)
      if [[ $# -lt 2 ]]; then
        echo "missing value for --expect" >&2
        exit 2
      fi
      expect="$2"
      shift
      ;;
    --install-root)
      if [[ $# -lt 2 ]]; then
        echo "missing value for --install-root" >&2
        exit 2
      fi
      install_root="$2"
      shift
      ;;
    --help|-h)
      usage
      exit 0
      ;;
    *)
      echo "unknown argument: $1" >&2
      usage >&2
      exit 2
      ;;
  esac
  shift
done

script_dir=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
repo_root=$(git -C "$script_dir/.." rev-parse --show-toplevel)

if [[ "$(basename "$repo_root")" != "c2rust_port" ]]; then
  echo "refusing to sync from non-canonical repo: $repo_root" >&2
  exit 1
fi

cd "$repo_root"

if [[ "$pull" -eq 1 ]]; then
  git pull --ff-only origin main
fi

full_commit=$(git rev-parse HEAD)
short_commit=$(git rev-parse --short HEAD)

if [[ -n "$expect" && "$full_commit" != "$expect"* && "$short_commit" != "$expect"* ]]; then
  echo "unexpected c2rust_port commit: got $short_commit, expected $expect" >&2
  exit 1
fi

src="$repo_root/skills/$skill_name"
dest="$install_root/$skill_name"

if [[ ! -f "$src/SKILL.md" ]]; then
  echo "missing canonical skill: $src/SKILL.md" >&2
  exit 1
fi

if [[ -e "$dest" || -L "$dest" ]]; then
  resolved_dest=$(readlink -f "$dest")
else
  resolved_dest="$dest"
fi

for script in "$src"/scripts/*.sh; do
  bash -n "$script"
done

install -d "$resolved_dest" "$resolved_dest/agents" "$resolved_dest/references" "$resolved_dest/scripts"
install -m 644 "$src/SKILL.md" "$resolved_dest/SKILL.md"
install -m 644 "$src"/agents/* "$resolved_dest/agents/"
install -m 644 "$src"/references/* "$resolved_dest/references/"
install -m 755 "$src"/scripts/*.sh "$resolved_dest/scripts/"

if ! diff -qr "$src" "$resolved_dest"; then
  echo "installed skill differs from canonical source" >&2
  exit 1
fi

cat <<REPORT
c-to-rust-port skill synced
source_repo=$repo_root
source_commit=$short_commit
installed_path=$resolved_dest
status=ok
REPORT
