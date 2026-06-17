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
source_functions_tsv="$out_dir/source-functions.tsv"
missing_candidates_tsv="$out_dir/missing-candidates.tsv"
stub_candidates_tsv="$out_dir/stub-candidates.tsv"

jq -r '
  .functions[]
  | [
      .name,
      .location.file,
      .location.line_start,
      .location.line_end,
      (.metrics.loc_code // 0),
      (.metrics.branches // 0),
      (.metrics.calls_unique // 0),
      (.metrics.cyclomatic // 0),
      (.metrics.cognitive // 0)
    ]
  | @tsv
' "$source_json" > "$source_functions_tsv"

pick_stub_pair() {
  local in_partial=0
  local line rust_name source_name rust_loc source_loc leaf score reason
  local raw="$out_dir/stub-candidates.raw.tsv"
  : > "$raw"

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
        [[ "$rust_name" == "$active_fn" || "$source_name" == "$active_fn" || "$source_name" == *"::$active_fn" ]] || continue
      fi
      leaf=${source_name##*::}
      score=0
      reason=""
      if (( source_loc >= 8 && source_loc <= 180 )); then
        score=$((score + 80))
        reason="${reason}loc_fit,"
      elif (( source_loc >= 4 && source_loc <= 320 )); then
        score=$((score + 35))
        reason="${reason}loc_ok,"
      else
        score=$((score - 45))
        reason="${reason}loc_poor,"
      fi
      if (( rust_loc <= 12 )); then
        score=$((score + 25))
        reason="${reason}stub_short,"
      fi
      if [[ "$source_name" == *::* ]]; then
        score=$((score + 15))
        reason="${reason}method,"
      fi
      if [[ ${#leaf} -le 2 || "$leaf" =~ ^(operator|iterator|begin|end|size|empty|clear|reset|free|swap|new|delete|main|o|c|get|set|init|print|close|open|load|save|done|next|prev|read|write)$ ]]; then
        score=$((score - 70))
        reason="${reason}generic_name,"
      fi
      printf '%s\t%s\t%s\t%s\t%s\t%s\n' "$score" "${reason%,}" "$rust_name" "$source_name" "$rust_loc" "$source_loc" >> "$raw"
    fi
  done < "$missing_txt"

  if [[ -s "$raw" ]]; then
    sort -t $'\t' -k1,1nr -k6,6n "$raw" > "$stub_candidates_tsv"
    head -n 1 "$stub_candidates_tsv"
    return 0
  fi

  return 1
}

pick_missing_order_row() {
  local raw="$out_dir/missing-candidates.raw.tsv"
  awk -F'\t' -v order_csv="$order_csv" -v fn="$active_fn" '
    function leaf_name(name, x) { x = name; sub(/^.*::/, "", x); return x }
    function is_generic(name, leaf) {
      leaf = leaf_name(name)
      return length(leaf) <= 2 || leaf ~ /^(operator|iterator|begin|end|size|empty|clear|reset|free|swap|new|delete|main|o|c|get|set|init|print|close|open|load|save|done|next|prev|read|write)$/
    }
    function add_reason(reason, part) {
      if (reason == "") return part
      return reason "," part
    }
    FNR == NR {
      key = $2 ":" $3
      source[key] = $0
      next
    }
    END {
      OFS = "\t"
      while ((getline line < order_csv) > 0) {
        if (line ~ /^name,/) continue
        n = split(line, cols, ",")
        if (n < 6) continue
        order_name = cols[1]
        file = cols[2]
        start = cols[3]
        translated = toupper(cols[6])
        if (translated != "FALSE") continue
        if (file !~ /\.(c|cc|cpp|cxx|h|hh|hpp|hxx)$/) continue
        if (file ~ /\/scripts\/|\/test\/|\/tests\/|\/benchmark\//) continue
        key = file ":" start
        if (!(key in source)) continue
        split(source[key], f, "\t")
        name = f[1]
        if (fn != "" && name != fn && name !~ ("::" fn "$") && order_name != fn && order_name !~ ("::" fn "$")) continue
        end = f[4]
        loc = f[5] + 0
        branches = f[6] + 0
        calls = f[7] + 0
        cyclo = f[8] + 0
        cognitive = f[9] + 0
        score = 0
        reason = ""
        if (loc >= 8 && loc <= 180) {
          score += 80; reason = add_reason(reason, "loc_fit")
        } else if (loc >= 4 && loc <= 320) {
          score += 35; reason = add_reason(reason, "loc_ok")
        } else {
          score -= 45; reason = add_reason(reason, "loc_poor")
        }
        if (branches > 0 && branches <= 45) {
          score += 35; reason = add_reason(reason, "bounded_branches")
        } else if (branches == 0) {
          score -= 10; reason = add_reason(reason, "no_branches")
        } else {
          score -= 25; reason = add_reason(reason, "too_branchy")
        }
        if (calls > 0 && calls <= 80) {
          score += 15; reason = add_reason(reason, "has_calls")
        }
        if (name ~ /::/) {
          score += 15; reason = add_reason(reason, "method")
        }
        if (is_generic(name)) {
          score -= 70; reason = add_reason(reason, "generic_name")
        }
        if (file ~ /\.(h|hh|hpp|hxx)$/ && loc <= 6) {
          score -= 20; reason = add_reason(reason, "tiny_header")
        }
        print score, reason, name, file, start, end, loc, branches, calls, cyclo, cognitive
      }
    }
  ' "$source_functions_tsv" > "$raw"

  if [[ -s "$raw" ]]; then
    sort -t $'\t' -k1,1nr -k7,7n "$raw" > "$missing_candidates_tsv"
    head -n 1 "$missing_candidates_tsv"
    return 0
  fi

  return 1
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
selection_score=""
selection_reason=""

missing_row=""
stub_row=""
[[ "$repair_kind" == "stub" ]] || missing_row=$(pick_missing_order_row || true)
[[ "$repair_kind" == "missing" ]] || stub_row=$(pick_stub_pair || true)

if [[ "$repair_kind" == "missing" && -n "$missing_row" ]]; then
  IFS=$'\t' read -r selection_score selection_reason source_name source_file source_start source_end _loc _branches _calls _cyclo _cog <<< "$missing_row"
  kind=missing
elif [[ "$repair_kind" == "stub" && -n "$stub_row" ]]; then
  IFS=$'\t' read -r selection_score selection_reason rust_name source_name _rust_loc _source_loc <<< "$stub_row"
  kind=stub
elif [[ -n "$missing_row" && -n "$stub_row" ]]; then
  missing_score=${missing_row%%$'\t'*}
  stub_score=${stub_row%%$'\t'*}
  if (( stub_score > missing_score )); then
    IFS=$'\t' read -r selection_score selection_reason rust_name source_name _rust_loc _source_loc <<< "$stub_row"
    kind=stub
  else
    IFS=$'\t' read -r selection_score selection_reason source_name source_file source_start source_end _loc _branches _calls _cyclo _cog <<< "$missing_row"
    kind=missing
  fi
elif [[ -n "$missing_row" ]]; then
  IFS=$'\t' read -r selection_score selection_reason source_name source_file source_start source_end _loc _branches _calls _cyclo _cog <<< "$missing_row"
  kind=missing
elif [[ -n "$stub_row" ]]; then
  IFS=$'\t' read -r selection_score selection_reason rust_name source_name _rust_loc _source_loc <<< "$stub_row"
  kind=stub
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
  echo "selection_score=${selection_score:-missing}"
  echo "selection_reason=${selection_reason:-missing}"
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
  [[ -f "$missing_candidates_tsv" ]] && echo "- missing candidates: $missing_candidates_tsv"
  [[ -f "$stub_candidates_tsv" ]] && echo "- stub candidates: $stub_candidates_tsv"
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
  echo "selection_score=${selection_score:-missing}"
  echo "selection_reason=${selection_reason:-missing}"
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
  [[ -f "$missing_candidates_tsv" ]] && echo "- $missing_candidates_tsv"
  [[ -f "$stub_candidates_tsv" ]] && echo "- $stub_candidates_tsv"
  [[ -f "$source_function_json" ]] && echo "- $source_function_json"
  [[ -f "$source_snippet" ]] && echo "- $source_snippet"
  [[ -f "$rust_function_json" ]] && echo "- $rust_function_json"
  [[ -f "$rust_snippet" ]] && echo "- $rust_snippet"
  [[ -f "$rust_candidates" ]] && echo "- $rust_candidates"
} > "$packet"

echo "$summary"
[[ "$status" == ready_to_implement ]] || exit 2
