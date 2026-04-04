#!/usr/bin/env bash
# Refreshes vendored tree-sitter grammar sources from upstream repos.
# Run from the tools/recon/ directory.
set -euo pipefail

VENDOR_DIR="$(cd "$(dirname "$0")/.." && pwd)/vendor"
TMP_DIR="$(mktemp -d)"
trap 'rm -rf "$TMP_DIR"' EXIT

clone_and_copy() {
    local repo="$1"
    local src_subdir="$2"
    local dest_name="$3"

    echo "Updating $dest_name from $repo..."
    git clone --depth 1 "$repo" "$TMP_DIR/$dest_name" 2>/dev/null

    local src="$TMP_DIR/$dest_name/$src_subdir/src"
    local dest="$VENDOR_DIR/$dest_name"

    rm -rf "$dest"
    mkdir -p "$dest"

    cp "$src/parser.c" "$dest/"
    [ -f "$src/scanner.c" ] && cp "$src/scanner.c" "$dest/"
    [ -f "$src/tag.h" ] && cp "$src/tag.h" "$dest/"
    cp -r "$src/tree_sitter" "$dest/"

    echo "  -> $dest"
}

clone_and_copy "https://github.com/tree-sitter/tree-sitter-javascript.git" "." "javascript"

# TypeScript repo contains both typescript/ and tsx/ grammars plus a shared common/ dir.
TS_REPO="$TMP_DIR/tree-sitter-typescript"
echo "Updating typescript + tsx from tree-sitter-typescript..."
git clone --depth 1 "https://github.com/tree-sitter/tree-sitter-typescript.git" "$TS_REPO" 2>/dev/null
for lang in typescript tsx; do
    src="$TS_REPO/$lang/src"
    dest="$VENDOR_DIR/$lang"
    rm -rf "$dest"
    mkdir -p "$dest"
    cp "$src/parser.c" "$dest/"
    [ -f "$src/scanner.c" ] && cp "$src/scanner.c" "$dest/"
    cp -r "$src/tree_sitter" "$dest/"
    echo "  -> $dest"
done

# Copy shared scanner header used by TypeScript/TSX scanners.
# The scanner.c files include "../../common/scanner.h", which resolves
# relative to vendor/{typescript,tsx}/ → common/ at the project root.
COMMON_DIR="$(cd "$(dirname "$0")/.." && pwd)/common"
mkdir -p "$COMMON_DIR"
cp "$TS_REPO/common/scanner.h" "$COMMON_DIR/"
echo "  -> $COMMON_DIR/scanner.h"

clone_and_copy "https://github.com/tree-sitter-grammars/tree-sitter-svelte.git" "." "svelte"

echo "Done. Vendored grammars updated."
