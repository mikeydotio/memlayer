#!/bin/bash
set -e

echo "Running memlayer migrations..."
for f in /docker-entrypoint-initdb.d/migrations/*.sql; do
    echo "  Applying: $(basename "$f")"
    psql -v ON_ERROR_STOP=1 --username "$POSTGRES_USER" --dbname "$POSTGRES_DB" -f "$f"
done
echo "Migrations complete."
