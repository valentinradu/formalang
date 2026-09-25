#!/usr/bin/env bash
# Run the cargo-fuzz targets.
#
# Usage:
#   scripts/fuzz.sh                 # every target, 60 s each
#   scripts/fuzz.sh parse           # one target, 60 s
#   scripts/fuzz.sh parse 600       # one target, 600 s
#
# `cargo fuzz` needs a nightly toolchain:
#   rustup toolchain install nightly
#   cargo install cargo-fuzz --locked
#
# The live corpus lives in `fuzz/corpus/<target>` and is not checked
# in. The curated seeds in `fuzz/seeds/text/` are, and so are the
# example programs; both are passed as extra corpus directories to the
# targets that take source text, so a fresh clone starts from real
# `FormaLang` source instead of noise.
#
# `-timeout=10` turns a slow input into a reported finding. The parser
# was exponential in nesting depth once, and a hang is how that showed.
# Each phase must take time that grows with the length of the input, so
# a slow input is a defect, not a slow machine.
set -euo pipefail

cd "$(dirname "$0")/.."

ALL_TARGETS=(lex parse compile program ir_json report)
TARGETS=("${1:-}")
DURATION="${2:-60}"

if [ -z "${1:-}" ]; then
    TARGETS=("${ALL_TARGETS[@]}")
fi

for target in "${TARGETS[@]}"; do
    echo "=== fuzzing ${target} for ${DURATION}s ==="
    mkdir -p "fuzz/corpus/${target}"

    # The byte-level targets take source text, so both the curated
    # seeds and the example programs are valid inputs for them.
    # `program` and `ir_json` take structured input, and `report` takes
    # a pair of strings; seeding those with plain text would only waste
    # the fuzzer's budget.
    extra=()
    case "${target}" in
        lex | parse | compile) extra+=("fuzz/seeds/text" "examples") ;;
        *) ;;
    esac

    cargo +nightly fuzz run "${target}" \
        "fuzz/corpus/${target}" "${extra[@]}" -- \
        "-max_total_time=${DURATION}" \
        -timeout=10 \
        -rss_limit_mb=4096
done

echo "=== done; crash artifacts (if any) are under fuzz/artifacts/ ==="
