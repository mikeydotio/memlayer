#!/usr/bin/env bash
# Post-bump hook: deploy server to Fly.io after version bump
set -euo pipefail

echo "Deploying memlayer server ${NEW_VERSION} to Fly.io..."

cd "${PROJECT_DIR:-$(git rev-parse --show-toplevel)}"

if ! command -v flyctl &>/dev/null; then
    echo "WARNING: flyctl not found, skipping deploy"
    exit 0
fi

flyctl deploy --now 2>&1

echo "Deploy complete: ${NEW_VERSION}"
