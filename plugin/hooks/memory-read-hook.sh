#!/usr/bin/env bash
# Memlayer PreToolUse hook — augments memory file reads with cross-session search
#
# Input:  JSON on stdin from Claude Code PreToolUse event
# Output: JSON on stdout with systemMessage (or nothing for non-memory files)
#
# Fast path: exits immediately with no output for non-memory file reads.
# Only runs memlayer search when a memory file is being read.

set -uo pipefail

# ── Read stdin ─────────────────────────────────────────────────────
INPUT="$(cat)" || exit 0

# ── Extract file_path ──────────────────────────────────────────────
FILE_PATH="$(printf '%s' "$INPUT" | jq -r '.tool_input.file_path // empty' 2>/dev/null)" || exit 0
[[ -z "$FILE_PATH" ]] && exit 0

# ── Fast path: skip non-memory files ──────────────────────────────
case "$FILE_PATH" in
  */memory/MEMORY.md|*/memory/*.md)
    ;; # matched — continue to memlayer search
  *)
    exit 0 ;; # not a memory file — pass through silently
esac

# ── Check if memlayer CLI is available ────────────────────────────
MEMLAYER_BIN="${HOME}/.local/bin/memlayer"
if [[ ! -x "$MEMLAYER_BIN" ]]; then
  MEMLAYER_BIN="$(command -v memlayer 2>/dev/null || true)"
  [[ -z "$MEMLAYER_BIN" ]] && exit 0
fi

# ── Load credentials if not in env ────────────────────────────────
if [[ -z "${MEMLAYER_AUTH_TOKEN:-}" || -z "${MEMLAYER_SERVER_URL:-}" ]]; then
  ENV_FILE="${HOME}/.config/memlayer/env"
  if [[ -f "$ENV_FILE" ]]; then
    # shellcheck disable=SC1090
    source "$ENV_FILE" 2>/dev/null || true
  fi
fi
export MEMLAYER_AUTH_TOKEN MEMLAYER_SERVER_URL

# ── Derive project path from cwd ─────────────────────────────────
CWD="$(printf '%s' "$INPUT" | jq -r '.cwd // empty' 2>/dev/null)" || true
PROJECT_FLAG=""
if [[ -n "${CWD:-}" ]]; then
  PROJECT_FLAG="--project $CWD"
fi

# ── Run memlayer search ───────────────────────────────────────────
STDERR_FILE="$(mktemp 2>/dev/null || echo /tmp/memlayer_stderr_$$)"
RESULTS="$("$MEMLAYER_BIN" search "recent work, decisions, and context" $PROJECT_FLAG --limit 5 --format text 2>"$STDERR_FILE")" || true
STDERR="$(cat "$STDERR_FILE" 2>/dev/null || true)"
rm -f "$STDERR_FILE" 2>/dev/null

# ── Check for version warnings in stderr ──────────────────────────
VERSION_WARNING=""
if printf '%s' "$STDERR" | grep -qiE 'version.*incompatible|upgrade.*required|read.only'; then
  VERSION_WARNING="[memlayer] WARNING: Version issue detected. Run 'memlayer status' for details.
${STDERR}
"
fi

# ── Return empty if no results and no warnings ────────────────────
[[ -z "$RESULTS" && -z "$VERSION_WARNING" ]] && exit 0

# ── Emit systemMessage with search results ────────────────────────
MSG="${VERSION_WARNING}[memlayer] Cross-session memory — recent context from past sessions:

${RESULTS}

Use \`memlayer search \"<specific query>\"\` for targeted searches if the above isn't relevant to the current task."

JSON_MSG="$(printf '%s' "$MSG" | jq -Rs '.')"
printf '{"systemMessage":%s}\n' "$JSON_MSG"

exit 0
