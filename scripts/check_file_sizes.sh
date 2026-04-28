#!/usr/bin/env bash
# Enforce a 500-line ceiling on src/**/*.rs files.
#
# Clippy has no per-file LOC lint (`too_many_lines` is per-function). This
# script is the equivalent gate. An allowlist records files temporarily
# above 500 with their current line count; the gate fails if a non-allowed
# file exceeds 500, OR if an allowed file grows beyond its recorded count.
# Goal: empty allowlist.
set -euo pipefail

LIMIT=500
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
ALLOWLIST="$ROOT/scripts/oversized_allowlist.txt"

declare -A allowed
if [[ -f "$ALLOWLIST" ]]; then
    while IFS= read -r line; do
        [[ -z "$line" || "$line" =~ ^[[:space:]]*# ]] && continue
        max=$(awk '{print $1}' <<< "$line")
        path=$(awk '{print $2}' <<< "$line")
        allowed["$path"]=$max
    done < "$ALLOWLIST"
fi

fail=0
shrunk=()
while IFS= read -r -d '' file; do
    rel="${file#"$ROOT"/}"
    lines=$(wc -l < "$file")
    if [[ -n "${allowed[$rel]+x}" ]]; then
        cap="${allowed[$rel]}"
        if (( lines > cap )); then
            echo "FAIL: $rel has $lines lines, exceeds allowlist cap of $cap" >&2
            fail=1
        elif (( lines <= LIMIT )); then
            shrunk+=("$rel ($lines lines)")
        elif (( lines < cap )); then
            echo "INFO: $rel shrank from $cap to $lines — tighten allowlist" >&2
        fi
    elif (( lines > LIMIT )); then
        echo "FAIL: $rel has $lines lines, exceeds limit of $LIMIT" >&2
        fail=1
    fi
done < <(find "$ROOT/src" -name '*.rs' -type f -print0)

if (( ${#shrunk[@]} > 0 )); then
    echo "INFO: these allowlisted files are now ≤ $LIMIT and can be removed from the allowlist:" >&2
    printf '  - %s\n' "${shrunk[@]}" >&2
fi

exit "$fail"
