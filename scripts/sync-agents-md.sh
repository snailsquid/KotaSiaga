#!/usr/bin/env bash
# Pull the shared AGENTS.md from Google Drive into the root of the current
# repo/worktree. Never fails the caller: on any problem it keeps the existing
# AGENTS.md and prints a warning to stderr.
#
# Auth (the Drive file is shared to the ITB domain only), first one that works wins:
#   1. GDRIVE_TOKEN env var      — an OAuth access token with drive.readonly scope
#   2. rclone remote             — set AGENTS_MD_RCLONE_REMOTE (default "itb-drive")
#   3. anonymous public download — works only if the file is "Anyone with the link"
set -u

FILE_ID="${AGENTS_MD_FILE_ID:-1Dy8hllbLEwAgWp1ZocUuJT8zclPaOsjR}"
REMOTE="${AGENTS_MD_RCLONE_REMOTE:-itb-drive}"

ROOT="$(git -C "${1:-$PWD}" rev-parse --show-toplevel 2>/dev/null || echo "${1:-$PWD}")"
DEST="$ROOT/AGENTS.md"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT
OUT="$TMP/AGENTS.md"

warn() { echo "sync-agents-md: $*" >&2; }

valid() {
  # Non-empty, not an HTML sign-in/error page, looks like the AGENTS.md doc
  [ -s "$OUT" ] && ! head -c 512 "$OUT" | grep -qi '<!doctype html\|<html' \
    && head -n 5 "$OUT" | grep -q '^# '
}

fetched=""
if [ -n "${GDRIVE_TOKEN:-}" ]; then
  curl -fsSL --max-time 15 -H "Authorization: Bearer $GDRIVE_TOKEN" \
    -o "$OUT" "https://www.googleapis.com/drive/v3/files/$FILE_ID?alt=media" 2>/dev/null \
    && valid && fetched="token"
fi
if [ -z "$fetched" ] && command -v rclone >/dev/null 2>&1 \
   && rclone listremotes 2>/dev/null | grep -qx "$REMOTE:"; then
  rm -f "$OUT"
  rclone --contimeout 10s --timeout 20s backend copyid "$REMOTE:" "$FILE_ID" "$OUT" 2>/dev/null \
    && valid && fetched="rclone"
fi
if [ -z "$fetched" ]; then
  rm -f "$OUT"
  curl -fsSL --max-time 15 -o "$OUT" \
    "https://drive.google.com/uc?export=download&id=$FILE_ID" 2>/dev/null \
    && valid && fetched="public"
fi

if [ -z "$fetched" ]; then
  warn "could not download AGENTS.md (no auth or no network); keeping existing copy."
  exit 0
fi

if [ -f "$DEST" ] && cmp -s "$OUT" "$DEST"; then
  exit 0
fi
cp "$OUT" "$DEST.tmp" && mv "$DEST.tmp" "$DEST"
warn "AGENTS.md updated from Google Drive (via $fetched)."
exit 0
