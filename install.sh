#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SKILLS_SRC="$SCRIPT_DIR/skills"
SKILLS_DST="$HOME/.claude/skills"
TIMESTAMP="$(date +%Y%m%d-%H%M%S)"
BACKUP_DIR="$HOME/.claude/skills-backup-$TIMESTAMP"
BACKED_UP=false

RECON_REPO="Corvalis-LLC/corvalis-skills"
RECON_BIN_DIR="$HOME/.claude/bin"
RECON_BIN="$RECON_BIN_DIR/corvalis-recon"

# --- Skills Installation ---

if [ ! -d "$SKILLS_SRC" ]; then
  echo "Error: skills/ directory not found in $SCRIPT_DIR"
  exit 1
fi

mkdir -p "$SKILLS_DST"

for skill_dir in "$SKILLS_SRC"/*/; do
  skill_name="$(basename "$skill_dir")"
  target="$SKILLS_DST/$skill_name"

  # Back up existing skill if it exists and isn't already a symlink to us
  if [ -e "$target" ]; then
    if [ -L "$target" ] && [ "$(readlink "$target")" = "$skill_dir" ]; then
      echo "  skip: $skill_name (already linked)"
      continue
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
done

echo ""
echo "Done. Skills linked to $SKILLS_DST"
if [ "$BACKED_UP" = true ]; then
  echo "Backups saved to $BACKUP_DIR"
fi

# --- Recon Binary Installation ---

detect_platform() {
  local os arch
  os="$(uname -s)"
  arch="$(uname -m)"

  case "$os" in
    Darwin)
      case "$arch" in
        arm64) echo "darwin-arm64" ;;
        x86_64) echo "darwin-x64" ;;
        *)
          echo "unsupported" >&2
          return 1
          ;;
      esac
      ;;
    Linux)
      case "$arch" in
        x86_64) echo "linux-x64" ;;
        *)
          echo "unsupported" >&2
          return 1
          ;;
      esac
      ;;
    MINGW*|MSYS*|CYGWIN*)
      echo "windows-x64"
      ;;
    *)
      echo "unsupported" >&2
      return 1
      ;;
  esac
}

fetch_latest_recon_tag() {
  # Uses GitHub API to find the latest recon-v* release tag
  local api_url="https://api.github.com/repos/$RECON_REPO/releases?per_page=20"
  local response

  response="$(curl -fsSL --max-time 10 "$api_url" 2>/dev/null)" || return 1

  # Extract the first tag_name matching recon-v* from the JSON array
  # Uses grep + sed to avoid jq dependency
  echo "$response" \
    | grep -o '"tag_name"[[:space:]]*:[[:space:]]*"recon-v[^"]*"' \
    | head -1 \
    | sed 's/.*"recon-v\([^"]*\)".*/\1/'
}

get_installed_recon_version() {
  if [ -x "$RECON_BIN" ]; then
    "$RECON_BIN" --version 2>/dev/null | sed 's/^corvalis-recon //' || echo ""
  else
    echo ""
  fi
}

install_recon() {
  echo ""
  echo "--- corvalis-recon ---"

  local platform
  platform="$(detect_platform 2>/dev/null)" || {
    echo "  skip: unsupported platform ($(uname -s)/$(uname -m))"
    return 0
  }

  # Check for curl
  if ! command -v curl >/dev/null 2>&1; then
    echo "  skip: curl not found (required for recon download)"
    return 0
  fi

  local latest_version
  latest_version="$(fetch_latest_recon_tag)" || {
    echo "  skip: unable to fetch latest recon release (network error or no releases)"
    return 0
  }

  if [ -z "$latest_version" ]; then
    echo "  skip: no recon releases found"
    return 0
  fi

  local installed_version
  installed_version="$(get_installed_recon_version)"

  if [ "$installed_version" = "$latest_version" ]; then
    echo "  skip: corvalis-recon v$latest_version (already installed)"
    return 0
  fi

  if [ -n "$installed_version" ]; then
    echo "  upgrading corvalis-recon v$installed_version -> v$latest_version"
  else
    echo "  installing corvalis-recon v$latest_version"
  fi

  local artifact_name="corvalis-recon-$platform"
  if [ "$platform" = "windows-x64" ]; then
    artifact_name="corvalis-recon-windows-x64.exe"
  fi

  local download_url="https://github.com/$RECON_REPO/releases/download/recon-v$latest_version/$artifact_name"

  mkdir -p "$RECON_BIN_DIR"

  local tmp_file
  tmp_file="$(mktemp "$RECON_BIN_DIR/corvalis-recon.XXXXXX")"

  if curl -fsSL --max-time 60 -o "$tmp_file" "$download_url" 2>/dev/null; then
    chmod +x "$tmp_file"
    mv "$tmp_file" "$RECON_BIN"
    echo "  installed: corvalis-recon v$latest_version -> $RECON_BIN"
  else
    rm -f "$tmp_file"
    echo "  warning: unable to download corvalis-recon from $download_url"
    echo "  the skills installation completed successfully; recon is optional"
    return 0
  fi
}

install_recon
