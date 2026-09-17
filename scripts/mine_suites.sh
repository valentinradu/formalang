#!/usr/bin/env bash
#
# Clone the four reference test suites and print the rules each one
# enforces, ranked by how often it appears.
#
# The suites are other languages' source code. They do not run against
# FormaLang and they are not vendored: they are read, not built. What
# comes out of them is a ranked taxonomy — the set of mistakes a mature
# compiler spends its checks on — and that ranking is the value. It is
# not the same as what seems worth testing from memory.
#
# The revisions are pinned so a re-run gives the same ranking. Bump them
# on purpose, not by accident.
#
# Usage:
#   scripts/mine_suites.sh                 # clone (if needed) and rank
#   scripts/mine_suites.sh --clean         # delete the clones
#   SUITES_DIR=/path scripts/mine_suites.sh
#
# The clones need about 120 MB and land in $SUITES_DIR, which defaults
# to a directory under the system temporary directory. They are outside
# the repository on purpose: each suite carries its own licence (Rust
# MIT/Apache-2.0, Swift Apache-2.0 with the LLVM exception, TypeScript
# Apache-2.0, Go BSD-3-Clause), and redistributing them would mean
# carrying four licence files for material nothing here builds against.

set -euo pipefail

SUITES_DIR="${SUITES_DIR:-${TMPDIR:-/tmp}/formalang-reference-suites}"

# repository|revision|sparse paths…
SUITES=(
  "https://github.com/rust-lang/rust|e15ceccfc6209c15b6c4bc6352f6ec6bfe579eaa|tests/ui"
  "https://github.com/microsoft/TypeScript|a5e123d9e0690fcea92878ea8a0a382922009fc9|tests/cases/conformance"
  "https://github.com/swiftlang/swift|548831a34a0648ad556b430c811b7c3ece5806dc|test/Constraints test/decl test/expr test/Generics test/stmt test/Sema"
  "https://github.com/golang/go|c0631ec996208f8741f50e5ff0f24e40a4f8e900|test"
)

if [ "${1:-}" = "--clean" ]; then
  rm -rf "$SUITES_DIR"
  echo "removed $SUITES_DIR"
  exit 0
fi

mkdir -p "$SUITES_DIR"

for entry in "${SUITES[@]}"; do
  IFS='|' read -r url revision paths <<<"$entry"
  name="$(basename "$url")"
  target="$SUITES_DIR/$name"

  if [ -d "$target/.git" ] && [ "$(git -C "$target" rev-parse HEAD)" = "$revision" ]; then
    echo "== $name already at $revision"
    continue
  fi

  echo "== cloning $name at $revision"
  rm -rf "$target"
  # A blobless, treeless, checkout-less clone fetches only what the
  # sparse paths need. A full clone of any of these is several GB.
  git clone --quiet --filter=blob:none --no-checkout "$url" "$target"
  git -C "$target" sparse-checkout init --cone
  # shellcheck disable=SC2086 # paths is a deliberate word list
  git -C "$target" sparse-checkout set $paths
  git -C "$target" checkout --quiet "$revision"
done

echo
echo "=============================================================="
echo " Reference suites in $SUITES_DIR"
echo " Files: $(find "$SUITES_DIR" -type f \( -name '*.rs' -o -name '*.ts' -o -name '*.swift' -o -name '*.go' \) | wc -l)"
echo "=============================================================="

# Each suite marks the error a case expects, next to the line that
# causes it. Extracting those marks gives the rules the language
# enforces; counting them ranks the rules by how much the compiler
# cares. Names and numbers inside a message are folded together so the
# same rule does not split across many rows.
fold_detail() {
  sed "s/\`[^\`]*\`/\`X\`/g; s/'[^']*'/'X'/g; s/[0-9]\+/N/g; s/ *$//"
}

echo
echo "--- Rust: //~ ERROR ---"
grep -rhoP '//~\^*\s*ERROR\s+\K.*' "$SUITES_DIR/rust/tests/ui/" 2>/dev/null |
  fold_detail | sort | uniq -c | sort -rn | head -40 || true

echo
echo "--- Swift: expected-error {{...}} ---"
grep -rhoP 'expected-error\s*[^{]*\{\{\K[^}]*' "$SUITES_DIR/swift/test/" 2>/dev/null |
  fold_detail | sort | uniq -c | sort -rn | head -40 || true

echo
echo "--- Go: // ERROR \"...\" ---"
grep -rhoP '//\s*ERROR\s+"\K[^"]*' "$SUITES_DIR/go/test/" 2>/dev/null |
  fold_detail | sort | uniq -c | sort -rn | head -40 || true

echo
echo "--- TypeScript: conformance areas by case count ---"
# TypeScript keeps its expected errors in separate baseline files, so
# rank the areas instead. The directory names are the taxonomy.
find "$SUITES_DIR/TypeScript/tests/cases/conformance" -mindepth 2 -maxdepth 2 -type d 2>/dev/null |
  while read -r dir; do
    printf '%7d %s\n' "$(find "$dir" -name '*.ts' | wc -l)" \
      "$(echo "$dir" | sed "s|$SUITES_DIR/TypeScript/tests/cases/conformance/||")"
  done | sort -rn | head -40 || true

echo
echo "Read a row as: this many cases in that suite guard this rule."
echo "Drop what FormaLang has no equivalent for; each remaining row is"
echo "a rule to check. See TESTING.md, 'Mining another language's suite'."
