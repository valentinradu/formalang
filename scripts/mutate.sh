#!/usr/bin/env bash
#
# Ask whether the tests would notice if the compiler were wrong.
#
# Every other check in this repository asks whether the compiler is
# right. This one asks the question underneath: change the compiler so
# it is wrong, and see whether any test fails. A change nothing
# notices — a "surviving mutant" — marks a line the suite does not
# really cover, whatever the coverage report says.
#
# The question is worth asking because this suite has been wrong about
# itself before. `test_overload_in_impl_block` declared two methods of
# one name, compiled them, and asserted nothing; it passed for as long
# as method overloading was impossible. Coverage counted those lines as
# covered. A mutation would have survived and said otherwise.
#
# Usage:
#   scripts/mutate.sh                       # the modules most worth checking
#   scripts/mutate.sh src/semantic          # a directory or a file
#   scripts/mutate.sh --all                 # everything; hours
#
# The whole tree is about 2600 mutants and each needs a test run, so a
# full pass takes hours. Run a directory at a time, or leave --all to a
# nightly.

set -euo pipefail

if ! command -v cargo-mutants >/dev/null; then
  echo "cargo-mutants is not installed. Install it with:" >&2
  echo "    cargo install cargo-mutants --locked" >&2
  exit 1
fi

# Per-mutant timeout. A mutation can turn a loop into one that never
# ends, and that has to be told apart from a slow build.
TIMEOUT="${MUTANT_TIMEOUT:-300}"
OUT="${MUTANT_OUT:-mutants.out}"

# The modules where a surviving mutant matters most: the rules that
# decide whether a program is accepted, and the passes that rewrite it
# before a backend sees it.
DEFAULT_TARGETS=(
  src/semantic/validation
  src/semantic/trait_check
  src/ir/lower/expr
  src/ir/monomorphise
  src/ir/overload.rs
)

args=()
if [ "$#" -eq 0 ]; then
  for t in "${DEFAULT_TARGETS[@]}"; do
    args+=(-f "$t")
  done
elif [ "$1" = "--all" ]; then
  args=()
else
  for t in "$@"; do
    args+=(-f "$t")
  done
fi

echo "Mutating. A surviving mutant is a line no test disagrees with."
echo

cargo mutants --timeout "$TIMEOUT" --output "$OUT" "${args[@]}" || true

echo
echo "=============================================================="
if [ -s "$OUT/missed.txt" ]; then
  echo " Survivors — each is a change no test noticed:"
  echo
  sed 's/^/   /' "$OUT/missed.txt"
  echo
  echo " For each one, either write a test that tells the difference,"
  echo " or satisfy yourself the line cannot matter and say why."
else
  echo " No survivors in this run."
fi
echo "=============================================================="
echo " Full detail: $OUT/outcomes.json"
