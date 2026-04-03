#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SKILLS_SRC="$SCRIPT_DIR/skills"
SKILLS_DST="$HOME/.codex/skills"
TIMESTAMP="$(date +%Y%m%d-%H%M%S)"
BACKUP_DIR="$HOME/.codex/skills-backup-$TIMESTAMP"
BACKED_UP=false

REQUIRED_SKILLS=(
  "verify"
  "auto-sanity"
)

OPTIONAL_SKILLS=(
  "codex-validation"
  "auto-testability"
)

if [ ! -d "$SKILLS_SRC" ]; then
  echo "Error: skills/ directory not found in $SCRIPT_DIR"
  exit 1
fi

mkdir -p "$SKILLS_DST"

link_skill() {
  local skill_name="$1"
  local skill_dir="$SKILLS_SRC/$skill_name"
  local target="$SKILLS_DST/$skill_name"

  if [ ! -d "$skill_dir" ]; then
    echo "  skip: $skill_name (missing from repo)"
    return
  fi

  if [ -e "$target" ]; then
    if [ -L "$target" ] && [ "$(readlink "$target")" = "$skill_dir" ]; then
      echo "  skip: $skill_name (already linked)"
      return
    fi
    if [ "$BACKED_UP" = false ]; then
      mkdir -p "$BACKUP_DIR"
      BACKED_UP=true
    fi
    echo "  backup: $skill_name -> $BACKUP_DIR/$skill_name"
    mv "$target" "$BACKUP_DIR/$skill_name"
  fi

  ln -s "$skill_dir" "$target"
  echo "  linked: $skill_name"
}

echo "Installing recommended Codex companion skills:"
for skill_name in "${REQUIRED_SKILLS[@]}"; do
  link_skill "$skill_name"
done

echo ""
echo "Optional Codex companion skills available:"
for skill_name in "${OPTIONAL_SKILLS[@]}"; do
  echo "  - $skill_name"
done

echo ""
echo "Done. Codex companion skills linked to $SKILLS_DST"
if [ "$BACKED_UP" = true ]; then
  echo "Backups saved to $BACKUP_DIR"
fi
