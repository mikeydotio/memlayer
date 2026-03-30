#!/usr/bin/env bash
# MemLayer database backup script
# Usage: ./scripts/backup.sh [backup_dir]
#
# Performs a pg_dump of the memlayer database and rotates old backups.
# Intended for cron: 0 3 * * * /path/to/memlayer/scripts/backup.sh
#
# Environment variables:
#   DATABASE_URL    - PostgreSQL connection string (default: from .env)
#   BACKUP_DIR      - Override backup directory (default: ~/.local/share/memlayer/backups)
#   BACKUP_KEEP     - Number of backups to retain (default: 7)
#   COMPOSE_PROJECT - Docker compose project directory (default: script's parent dir)

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="${COMPOSE_PROJECT:-$(dirname "$SCRIPT_DIR")}"
BACKUP_DIR="${1:-${BACKUP_DIR:-$HOME/.local/share/memlayer/backups}}"
BACKUP_KEEP="${BACKUP_KEEP:-7}"
TIMESTAMP="$(date +%Y%m%d_%H%M%S)"
BACKUP_FILE="$BACKUP_DIR/memlayer_$TIMESTAMP.sql.gz"

mkdir -p "$BACKUP_DIR"

echo "MemLayer backup: $TIMESTAMP"

if [ -n "${DATABASE_URL:-}" ]; then
    # Direct pg_dump with connection string
    pg_dump "$DATABASE_URL" --no-owner --no-privileges | gzip > "$BACKUP_FILE"
else
    # Use docker compose to reach the DB container
    docker compose -f "$PROJECT_DIR/docker-compose.yml" exec -T db \
        pg_dump -U memlayer memlayer --no-owner --no-privileges | gzip > "$BACKUP_FILE"
fi

SIZE=$(du -h "$BACKUP_FILE" | cut -f1)
echo "Backup written: $BACKUP_FILE ($SIZE)"

# Rotate: keep only the most recent N backups
BACKUPS=($(ls -1t "$BACKUP_DIR"/memlayer_*.sql.gz 2>/dev/null))
if [ ${#BACKUPS[@]} -gt "$BACKUP_KEEP" ]; then
    for OLD in "${BACKUPS[@]:$BACKUP_KEEP}"; do
        echo "Rotating out: $(basename "$OLD")"
        rm -f "$OLD"
    done
fi

echo "Done. Retained ${BACKUP_KEEP} most recent backups."
