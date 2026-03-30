#!/usr/bin/env bash
# Post-bump hook: sync version across all components before deploy
set -euo pipefail

cd "${PROJECT_DIR:-$(git rev-parse --show-toplevel)}"

echo "Syncing version ${NEW_VERSION} across all components..."
bash scripts/sync-version.sh "${NEW_VERSION#v}"

# Stage the changes so they're included in the version commit
git add \
    daemon/Cargo.toml \
    cli-rs/Cargo.toml \
    memlayer-common/Cargo.toml \
    server/pyproject.toml \
    plugin/.claude-plugin/plugin.json \
    plugin/skills/memory/SKILL.md

echo "Version sync staged for commit"
