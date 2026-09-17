#!/usr/bin/env bash
#
# Mutation sweep for formalang, with progress.
#
#   scripts/mutate.sh                       # whole project
#   scripts/mutate.sh src/semantic          # one directory
#   scripts/mutate.sh src/ir/overload.rs    # one file
#
# Environment:
#   JOBS=12        concurrent mutants (default 12)
#   TMP=<dir>      where the working copies go (default ~/.cache/…, on disk)
#   TIMEOUT=300    seconds per mutant before it counts as a hang
#
# A note on JOBS. Each job runs its own `cargo build`, and cargo already
# uses every core, so JOBS=12 on a 12-core machine asks for far more
# threads than there are cores. It still finishes — the jobs spend much
# of their time waiting on one another's I/O — but if the machine
# becomes unusable, halve it. The wall-clock difference between 6 and 12
# is smaller than the number suggests.
#
# A note on disk. Each job keeps a full working copy with its own
# target/ directory. That is tens of gigabytes at twelve jobs, which is
# why TMP defaults to a path on real disk rather than wherever the
# system puts temporary files: on a machine where /tmp is RAM, twelve
# Rust builds there would take the machine down. The copies are removed
# when the run ends, including on Ctrl-C.

set -uo pipefail

# The repository root, found from this script rather than hard-coded.
PROJECT="${PROJECT:-$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)}"
JOBS="${JOBS:-12}"
TIMEOUT="${TIMEOUT:-300}"
TMP="${TMP:-$HOME/.cache/formalang-mutants}"
TARGET="${1:-}"

OUT="$TMP/out"
LOG="$TMP/run.log"

cleanup() {
  echo
  echo "cleaning working copies…"
  pkill -f cargo-mutants 2>/dev/null
  # Wait for them to go before deleting: a copy still being written to
  # comes back, and tens of gigabytes stay behind for the next run to
  # trip over.
  for _ in $(seq 20); do
    pgrep -f cargo-mutants >/dev/null 2>&1 || break
    sleep 1
  done
  pkill -9 -f cargo-mutants 2>/dev/null
  sleep 1
  rm -rf "$TMP"/cargo-mutants-*.tmp 2>/dev/null
  left=$(ls -d "$TMP"/cargo-mutants-*.tmp 2>/dev/null | wc -l)
  if [ "$left" -gt 0 ]; then
    echo "  $left copy(ies) would not delete; remove $TMP/cargo-mutants-*.tmp by hand" >&2
  fi
  echo "done. Results kept in $OUT"
}
trap cleanup EXIT INT TERM

command -v cargo-mutants >/dev/null || {
  echo "cargo-mutants is not installed:  cargo install cargo-mutants --locked" >&2
  exit 1
}

mkdir -p "$TMP"
rm -rf "$OUT" "$LOG"

# --- what are we mutating, and is there room -------------------------------
cd "$PROJECT" || exit 1

args=(-j "$JOBS" --timeout "$TIMEOUT" --output "$OUT")
scope="the whole project"
if [ -n "$TARGET" ] && [ "$TARGET" != "$PROJECT" ]; then
  args+=(-f "$TARGET")
  scope="$TARGET"
fi

total=$(cargo mutants --list ${TARGET:+-f "$TARGET"} 2>/dev/null | wc -l)
free_gb=$(df -BG --output=avail "$TMP" | tail -1 | tr -dc '0-9')

echo "=============================================================="
echo " Mutating $scope"
echo "   mutants   $total"
echo "   jobs      $JOBS"
echo "   copies in $TMP   ($free_gb GB free)"
echo "=============================================================="
echo
if [ "$free_gb" -lt 60 ]; then
  echo "Less than 60 GB free. Each job keeps its own target/ directory;" >&2
  echo "run 'cargo clean' first or lower JOBS." >&2
  exit 1
fi

# --- run -------------------------------------------------------------------
TMPDIR="$TMP" nohup cargo mutants "${args[@]}" > "$LOG" 2>&1 &
runner=$!
started=$(date +%s)
first_done_at=0
last_missed=0

printf '%-9s %-8s %-8s %-9s %-8s %-10s %s\n' \
  ELAPSED DONE CAUGHT MISSED UNVIABLE RATE ETA

while kill -0 "$runner" 2>/dev/null; do
  sleep 30
  [ -f "$OUT/mutants.out/outcomes.json" ] || continue

  read -r done caught missed unviable <<<"$(python3 - "$OUT/mutants.out/outcomes.json" <<'PY'
import json, sys
from collections import Counter
try:
    o = json.load(open(sys.argv[1])).get("outcomes", [])
except Exception:
    print("0 0 0 0"); raise SystemExit
c = Counter(x["summary"] for x in o)
# The baseline is a "Success" and is not a mutant.
print(len(o) - c.get("Success", 0), c.get("CaughtMutant", 0),
      c.get("MissedMutant", 0), c.get("Unviable", 0))
PY
)"

  now=$(date +%s); elapsed=$((now - started))
  # Time the mutants, not the baseline build that precedes them.
  # Averaging the build in makes the first estimates far too pessimistic.
  if [ "$done" -gt 0 ] && [ "$first_done_at" -eq 0 ]; then
    first_done_at=$now
  fi
  testing=$((now - first_done_at))
  if [ "$done" -gt 1 ] && [ "$testing" -gt 0 ]; then
    rate=$(awk "BEGIN{printf \"%.1f\", $done*60/$testing}")
    left=$((total - done))
    eta=$(awk "BEGIN{m=$left*$testing/$done/60; if (m<90) printf \"%dm\", m; else printf \"%.1fh\", m/60}")
  else
    rate="—"; eta="building…"
  fi
  printf '%-9s %-8s %-8s %-9s %-8s %-10s %s\n' \
    "$(printf '%dm%02ds' $((elapsed/60)) $((elapsed%60)))" \
    "$done/$total" "$caught" "$missed" "$unviable" "$rate/min" "$eta"

  # Print each survivor the moment it appears: they are the findings,
  # and there is no reason to wait hours to see the first one.
  if [ "$missed" -gt "$last_missed" ]; then
    python3 - "$OUT/mutants.out/outcomes.json" "$last_missed" <<'PY'
import json, sys
o = json.load(open(sys.argv[1])).get("outcomes", [])
seen = int(sys.argv[2])
missed = [x for x in o if x["summary"] == "MissedMutant"]
for x in missed[seen:]:
    m = x.get("scenario", {}).get("Mutant", {})
    line = m.get("span", {}).get("start", {}).get("line")
    fn = m.get("function", {}).get("function_name")
    print(f"    SURVIVED  {m.get('file')}:{line}  {fn} -> {m.get('replacement')}")
PY
    last_missed="$missed"
  fi
done

# --- report ----------------------------------------------------------------
echo
echo "=============================================================="
if [ -s "$OUT/mutants.out/missed.txt" ]; then
  n=$(wc -l < "$OUT/mutants.out/missed.txt")
  echo " $n survivor(s) — each is a change to the compiler that no test"
  echo " disagreed with. For each: write a test that tells the"
  echo " difference, or satisfy yourself the line cannot matter and"
  echo " say why in a comment."
  echo
  sed 's/^/   /' "$OUT/mutants.out/missed.txt"
else
  echo " No survivors."
fi
echo "=============================================================="
echo " Full detail: $OUT/mutants.out/outcomes.json"
echo " Run log:     $LOG"
