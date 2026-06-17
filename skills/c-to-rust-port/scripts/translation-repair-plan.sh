#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat >&2 <<'EOF'
usage: translation-repair-plan.sh <source-dir> <rust-dir> [out-dir]

Optional env:
  CCC_DIR=/path/to/existing/ccc-artifacts
  ACTIVE_FUNCTION=function_name
  REPAIR_KIND=auto|missing|stub
EOF
}

if [[ $# -lt 2 || $# -gt 3 ]]; then
  usage
  exit 2
fi

source_dir=$1
rust_dir=$2
out_dir=${3:-"$rust_dir/.port-work/translation-repair"}
repair_kind=${REPAIR_KIND:-auto}
active_fn=${ACTIVE_FUNCTION:-}

script_dir=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)

if ! command -v jq >/dev/null 2>&1; then
  echo "jq not found on PATH" >&2
  exit 127
fi

case "$repair_kind" in
  auto|missing|stub) ;;
  *)
    echo "REPAIR_KIND must be auto, missing, or stub" >&2
    exit 2
    ;;
esac

mkdir -p "$out_dir"

if [[ -n "${CCC_DIR:-}" ]]; then
  ccc_dir=$CCC_DIR
elif [[ -f "$rust_dir/.port-work/equivalence/ccc/source.json" ]]; then
  ccc_dir="$rust_dir/.port-work/equivalence/ccc"
elif [[ -f "$rust_dir/.port-work/ccc/source.json" ]]; then
  ccc_dir="$rust_dir/.port-work/ccc"
else
  ccc_dir="$out_dir/ccc"
fi

source_json="$ccc_dir/source.json"
rust_json="$ccc_dir/rust.json"
order_csv="$ccc_dir/order.csv"
missing_txt="$ccc_dir/missing.txt"

if [[ ! -f "$source_json" || ! -f "$rust_json" || ! -f "$order_csv" || ! -f "$missing_txt" ]]; then
  "$script_dir/ccc-brief.sh" "$source_dir" "$rust_dir" "$ccc_dir" >/dev/null
fi

source_json="$ccc_dir/source.json"
rust_json="$ccc_dir/rust.json"
order_csv="$ccc_dir/order.csv"
missing_txt="$ccc_dir/missing.txt"

pick_stub_pair() {
  local best_rust=""
  local best_source=""
  local best_loc=999999
  local in_partial=0
  local line rust_name source_name rust_loc source_loc

  while IFS= read -r line; do
    if [[ "$line" == Partial/stubs* ]]; then
      in_partial=1
      continue
    fi
    if [[ "$line" == Extra\ in\ Rust* ]]; then
      in_partial=0
    fi
    [[ "$in_partial" -eq 1 ]] || continue

    if [[ "$line" =~ ^[[:space:]]+~[[:space:]]+([^[:space:]]+)[[:space:]]\(rust\)[[:space:]]vs[[:space:]](.+)[[:space:]]\(other\):[[:space:]]rust[[:space:]]LOC[[:space:]]([0-9]+).*other[[:space:]]LOC[[:space:]]([0-9]+) ]]; then
      rust_name=${BASH_REMATCH[1]}
      source_name=${BASH_REMATCH[2]}
      rust_loc=${BASH_REMATCH[3]}
      source_loc=${BASH_REMATCH[4]}
      if [[ -n "$active_fn" ]]; then
        if [[ "$rust_name" == "$active_fn" || "$source_name" == "$active_fn" || "$source_name" == *"::$active_fn" ]]; then
          printf '%s\t%s\t%s\t%s\n' "$rust_name" "$source_name" "$rust_loc" "$source_loc"
          return 0
        fi
      elif [[ "$source_loc" -lt "$best_loc" ]]; then
        best_rust=$rust_name
        best_source=$source_name
        best_loc=$source_loc
      fi
    fi
  done < "$missing_txt"

  if [[ -n "$best_source" ]]; then
    printf '%s\t%s\t\t%s\n' "$best_rust" "$best_source" "$best_loc"
    return 0
  fi

  return 1
}

pick_missing_order_row() {
  if [[ -n "$active_fn" ]]; then
    awk -F, -v fn="$active_fn" '
      NR == 1 { next }
      toupper($6) != "FALSE" { next }
      $2 !~ /\.(c|cc|cpp|cxx|h|hh|hpp|hxx)$/ { next }
      $2 ~ /\/scripts\/|\/test\// { next }
      $1 == fn || $1 ~ ("::" fn "$") { print; exit }
    ' "$order_csv"
  else
    awk -F, '
      NR == 1 { next }
      toupper($6) != "FALSE" { next }
      $2 !~ /\.(c|cc|cpp|cxx|h|hh|hpp|hxx)$/ { next }
      $2 ~ /\/scripts\/|\/test\// { next }
      { print; exit }
    ' "$order_csv"
  fi
}

kind=""
source_name=""
rust_name=""
source_file=""
source_start=""
source_end=""
rust_file=""
rust_start=""
rust_end=""

if [[ "$repair_kind" != "stub" ]]; then
  missing_row=$(pick_missing_order_row || true)
  if [[ -n "$missing_row" ]]; then
    IFS=, read -r source_name source_file source_start _scc_id _scc_kind _translated <<< "$missing_row"
    kind=missing
  fi
fi

if [[ -z "$kind" && "$repair_kind" != "missing" ]]; then
  stub_row=$(pick_stub_pair || true)
  if [[ -n "$stub_row" ]]; then
    IFS=$'\t' read -r rust_name source_name _rust_loc _source_loc <<< "$stub_row"
    kind=stub
  fi
fi

if [[ -z "$kind" && -n "$active_fn" ]]; then
  source_name=$active_fn
  kind=manual
fi

source_function_json="$out_dir/source-function.json"
rust_function_json="$out_dir/rust-function.json"

if [[ "$kind" == "missing" ]]; then
  jq --arg file "$source_file" --argjson line "$source_start" '
    [ .functions[]
      | select(.location.file == $file and .location.line_start == $line)
      | {name, location, signature, metrics, calls, constants, types_used}
    ][0] // empty
  ' "$source_json" > "$source_function_json"
else
  jq --arg name "$source_name" '
    [ .functions[]
      | select(.name == $name or (.name | endswith("::" + $name)))
    ]
    | sort_by(.metrics.loc_code // 999999)
    | .[0]
    | if . then {name, location, signature, metrics, calls, constants, types_used} else empty end
  ' "$source_json" > "$source_function_json"
fi

if [[ ! -s "$source_function_json" ]]; then
  rm -f "$source_function_json"
fi

if [[ -f "$source_function_json" ]]; then
  source_name=$(jq -r '.name // ""' "$source_function_json")
  source_file=$(jq -r '.location.file // ""' "$source_function_json")
  source_start=$(jq -r '.location.line_start // ""' "$source_function_json")
  source_end=$(jq -r '.location.line_end // ""' "$source_function_json")
fi

if [[ -n "$rust_name" ]]; then
  jq --arg name "$rust_name" '
    [ .functions[]
      | select(.name == $name or (.name | endswith("::" + $name)))
    ]
    | sort_by(.metrics.loc_code // 999999)
    | .[0]
    | if . then {name, location, signature, metrics, calls, constants, types_used} else empty end
  ' "$rust_json" > "$rust_function_json"
elif [[ -n "$source_name" ]]; then
  simple_name=${source_name##*::}
  jq --arg name "$simple_name" '
    [ .functions[]
      | select(.name == $name or (.name | endswith("::" + $name)))
    ]
    | sort_by(.metrics.loc_code // 999999)
    | .[0]
    | if . then {name, location, signature, metrics, calls, constants, types_used} else empty end
  ' "$rust_json" > "$rust_function_json"
fi

if [[ -s "$rust_function_json" ]]; then
  rust_name=$(jq -r '.name // ""' "$rust_function_json")
  rust_file=$(jq -r '.location.file // ""' "$rust_function_json")
  rust_start=$(jq -r '.location.line_start // ""' "$rust_function_json")
  rust_end=$(jq -r '.location.line_end // ""' "$rust_function_json")
else
  rm -f "$rust_function_json"
fi

source_snippet="$out_dir/source-snippet.txt"
rust_snippet="$out_dir/rust-existing-snippet.txt"
rust_candidates="$out_dir/rust-candidates.txt"

if [[ -n "$source_file" && -n "$source_start" && -n "$source_end" && -r "$source_file" ]]; then
  sed -n "${source_start},${source_end}p" "$source_file" > "$source_snippet"
fi

if [[ -n "$rust_file" && -n "$rust_start" && -n "$rust_end" && -r "$rust_file" ]]; then
  sed -n "${rust_start},${rust_end}p" "$rust_file" > "$rust_snippet"
fi

simple_name=${rust_name:-${source_name##*::}}
if command -v rg >/dev/null 2>&1 && [[ -n "$simple_name" ]]; then
  set +e
  rg -n --fixed-strings "$simple_name" "$rust_dir" | head -80 > "$rust_candidates"
  set -e
fi

summary="$out_dir/SUMMARY.md"
packet="$out_dir/IMPLEMENTATION_PACKET.md"
rust_label=${rust_name:-missing}
rust_location_label=${rust_file:-missing}:${rust_start:-?}-${rust_end:-?}
if [[ "$kind" == "missing" && -n "$rust_name" ]]; then
  rust_label="nearest_candidate:$rust_name"
  rust_location_label="nearest_candidate:${rust_file:-missing}:${rust_start:-?}-${rust_end:-?}"
fi

if [[ -f "$source_function_json" ]]; then
  status=ready_to_implement
  first_blocker=none
  next_action="implement or replace the Rust function from source evidence, then rerun CCC before tracehash/gdb-tv"
else
  status=needs_manual_selection
  first_blocker="could not resolve active source function from CCC artifacts"
  next_action="set ACTIVE_FUNCTION to an exact CCC source name or choose a concrete row from order.csv/missing.txt"
fi

{
  echo "# Translation Repair Plan"
  echo
  echo "generated_at=$(date -u +%Y-%m-%dT%H:%M:%SZ)"
  echo "truth_policy=current CCC output plus current source tree are authoritative; repo ledgers and old packets are hints"
  echo "status=$status"
  echo "first_blocker=$first_blocker"
  echo "next_action=$next_action"
  echo
  echo "source=$source_dir"
  echo "rust=$rust_dir"
  echo "ccc=$ccc_dir"
  echo "kind=$kind"
  echo "active_function=${active_fn:-auto}"
  echo
  echo "## Selected Unit"
  echo "- source: ${source_name:-unknown} ${source_file:+($source_file:$source_start)}"
  if [[ -n "$rust_name" ]]; then
    if [[ "$kind" == "missing" ]]; then
      echo "- rust candidate: $rust_name ${rust_file:+($rust_file:$rust_start)}"
    else
      echo "- rust: $rust_name ${rust_file:+($rust_file:$rust_start)}"
    fi
  else
    echo "- rust: missing or unresolved"
  fi
  echo
  echo "## Artifacts"
  [[ -f "$source_function_json" ]] && echo "- source function json: $source_function_json"
  [[ -f "$source_snippet" ]] && echo "- source snippet: $source_snippet"
  [[ -f "$rust_function_json" ]] && echo "- rust function json: $rust_function_json"
  [[ -f "$rust_snippet" ]] && echo "- existing rust snippet: $rust_snippet"
  [[ -f "$rust_candidates" ]] && echo "- rust candidates: $rust_candidates"
  echo "- implementation packet: $packet"
} > "$summary"

{
  echo "# Implementation Packet"
  echo
  echo "status=$status"
  echo "kind=$kind"
  echo "source_function=${source_name:-unknown}"
  echo "source_location=${source_file:-unknown}:${source_start:-?}-${source_end:-?}"
  echo "rust_function=$rust_label"
  echo "rust_location=$rust_location_label"
  echo
  echo "## Use"
  echo "1. Read only the source snippet/function JSON and the nearest Rust candidate."
  echo "2. Preserve source control flow, constants, signedness, integer widths, output order, and fail-loud behavior."
  echo "3. If dependencies are missing, repair the smallest prerequisite function first with this same script."
  echo "4. After editing, rerun CCC and only then create behavior inputs or run tracehash/gdb-tv."
  echo
  echo "## Commands"
  echo '```bash'
  if [[ -n "$active_fn" ]]; then
    echo "ACTIVE_FUNCTION=\"$active_fn\" REPAIR_KIND=\"$repair_kind\" CCC_DIR=\"$ccc_dir\" \"$script_dir/translation-repair-plan.sh\" \"$source_dir\" \"$rust_dir\" \"$out_dir\""
  else
    echo "REPAIR_KIND=\"$repair_kind\" CCC_DIR=\"$ccc_dir\" \"$script_dir/translation-repair-plan.sh\" \"$source_dir\" \"$rust_dir\" \"$out_dir\""
  fi
  echo "\"$script_dir/ccc-brief.sh\" \"$source_dir\" \"$rust_dir\" \"$ccc_dir\""
  echo '```'
  echo
  echo "## Evidence Files"
  [[ -f "$source_function_json" ]] && echo "- $source_function_json"
  [[ -f "$source_snippet" ]] && echo "- $source_snippet"
  [[ -f "$rust_function_json" ]] && echo "- $rust_function_json"
  [[ -f "$rust_snippet" ]] && echo "- $rust_snippet"
  [[ -f "$rust_candidates" ]] && echo "- $rust_candidates"
} > "$packet"

echo "$summary"
[[ "$status" == ready_to_implement ]] || exit 2
