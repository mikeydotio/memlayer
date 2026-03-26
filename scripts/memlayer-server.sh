#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_DIR="$(dirname "$SCRIPT_DIR")"
source "$SCRIPT_DIR/lib.sh"

VERSION="1.0.0"

usage() {
    cat <<EOF
memlayer-server v${VERSION} — Claude Code Memory Layer (Server Admin)

Usage: memlayer-server <command> [options]

Commands:
    setup           Interactive setup wizard (server or client)
    status          Show service status and health
    backup          Backup database and response files
    restore         Restore from a backup archive
    forget          Delete entries by session or project
    verify          Check data integrity
    version         Show version info

Run 'memlayer-server <command> --help' for details.
EOF
}

# ── cmd_status ──────────────────────────────────────────────────────
cmd_status() {
    info "Checking memlayer status..."
    echo

    # Server
    if curl -sf --max-time 3 http://localhost:8420/health 2>/dev/null | python3 -m json.tool 2>/dev/null; then
        success "Server: running"
    else
        error "Server: not reachable"
    fi

    # Daemon
    if pgrep -f memlayer-daemon &>/dev/null; then
        success "Daemon: running (PID $(pgrep -f memlayer-daemon | head -1))"
    elif systemctl --user is-active memlayer-daemon &>/dev/null 2>&1; then
        success "Daemon: running (systemd)"
    else
        warn "Daemon: not running"
    fi

    # Database
    if docker exec memlayer-db pg_isready -U memlayer &>/dev/null 2>&1; then
        local count
        count=$(docker exec memlayer-db psql -U memlayer -d memlayer -tAc "SELECT COUNT(*) FROM memory_entries" 2>/dev/null || echo "?")
        success "Database: $count entries"
    else
        error "Database: not reachable"
    fi
}

# ── cmd_backup ──────────────────────────────────────────────────────
cmd_backup() {
    local output="${1:-memlayer-backup-$(date +%Y%m%d-%H%M%S).tar.gz}"
    local tmpdir
    tmpdir=$(mktemp -d)

    info "Creating backup..."

    # Database dump
    docker exec memlayer-db pg_dump -U memlayer -d memlayer --clean --if-exists > "$tmpdir/database.sql"
    success "Database dumped"

    # Response files
    local rf_vol
    rf_vol=$(docker volume inspect memlayer_response-files --format '{{.Mountpoint}}' 2>/dev/null || true)
    if [[ -n "$rf_vol" ]] && sudo test -d "$rf_vol"; then
        sudo cp -r "$rf_vol" "$tmpdir/response_files"
        success "Response files copied"
    else
        mkdir -p "$tmpdir/response_files"
        info "No response files found (or not accessible)"
    fi

    # Metadata
    echo "{\"version\":\"$VERSION\",\"created\":\"$(date -u +%Y-%m-%dT%H:%M:%SZ)\",\"hostname\":\"$(hostname)\"}" > "$tmpdir/backup-meta.json"

    # Archive
    tar -czf "$output" -C "$tmpdir" .
    rm -rf "$tmpdir"

    success "Backup created: $output ($(du -h "$output" | cut -f1))"
}

# ── cmd_restore ─────────────────────────────────────────────────────
cmd_restore() {
    local archive="${1:-}"
    if [[ -z "$archive" || ! -f "$archive" ]]; then
        error "Usage: memlayer-server restore <backup-file.tar.gz>"
        exit 1
    fi

    warn "This will replace ALL data in the database!"
    if ! confirm "Continue?" "n"; then
        exit 0
    fi

    local tmpdir
    tmpdir=$(mktemp -d)

    info "Extracting backup..."
    tar -xzf "$archive" -C "$tmpdir"

    if [[ ! -f "$tmpdir/database.sql" ]]; then
        error "Invalid backup: missing database.sql"
        rm -rf "$tmpdir"
        exit 1
    fi

    # Show backup metadata
    if [[ -f "$tmpdir/backup-meta.json" ]]; then
        info "Backup info: $(cat "$tmpdir/backup-meta.json")"
    fi

    # Restore database
    info "Restoring database..."
    docker exec -i memlayer-db psql -U memlayer -d memlayer < "$tmpdir/database.sql"
    success "Database restored"

    # Restore response files
    if [[ -d "$tmpdir/response_files" ]] && ls "$tmpdir/response_files"/* &>/dev/null 2>&1; then
        info "Restoring response files..."
        docker cp "$tmpdir/response_files/." memlayer-server:/data/response_files/
        success "Response files restored"
    fi

    rm -rf "$tmpdir"
    success "Restore complete"
}

# ── cmd_forget ──────────────────────────────────────────────────────
cmd_forget() {
    local session_id="" project_path=""
    while [[ $# -gt 0 ]]; do
        case "$1" in
            --session) session_id="$2"; shift 2 ;;
            --project) project_path="$2"; shift 2 ;;
            *) error "Unknown option: $1"; exit 1 ;;
        esac
    done

    if [[ -z "$session_id" && -z "$project_path" ]]; then
        error "Usage: memlayer-server forget --session <id> | --project <path>"
        exit 1
    fi

    if [[ -n "$session_id" ]]; then
        local count
        count=$(docker exec memlayer-db psql -U memlayer -d memlayer -tAc \
            "SELECT COUNT(*) FROM memory_entries WHERE session_id = '$session_id'")
        if [[ "$count" == "0" ]]; then
            info "No entries found for session $session_id"
            return
        fi
        warn "This will delete $count entries from session $session_id"
        if confirm "Continue?" "n"; then
            docker exec memlayer-db psql -U memlayer -d memlayer -c \
                "DELETE FROM memory_entries WHERE session_id = '$session_id'; DELETE FROM claude_sessions WHERE session_id = '$session_id';"
            success "Deleted $count entries"
        fi
    fi

    if [[ -n "$project_path" ]]; then
        local count
        count=$(docker exec memlayer-db psql -U memlayer -d memlayer -tAc \
            "SELECT COUNT(*) FROM memory_entries me JOIN claude_sessions cs ON me.session_id = cs.session_id WHERE cs.project_path = '$project_path'")
        if [[ "$count" == "0" ]]; then
            info "No entries found for project $project_path"
            return
        fi
        warn "This will delete $count entries from project $project_path"
        if confirm "Continue?" "n"; then
            docker exec memlayer-db psql -U memlayer -d memlayer -c \
                "DELETE FROM memory_entries WHERE session_id IN (SELECT session_id FROM claude_sessions WHERE project_path = '$project_path'); DELETE FROM claude_sessions WHERE project_path = '$project_path';"
            success "Deleted $count entries"
        fi
    fi
}

# ── cmd_verify ──────────────────────────────────────────────────────
cmd_verify() {
    info "Running data integrity checks..."
    local issues=0

    # Check 1: Database connectivity
    if docker exec memlayer-db pg_isready -U memlayer -d memlayer &>/dev/null; then
        success "Database: reachable"
    else
        error "Database: not reachable"
        issues=$((issues + 1))
    fi

    # Check 2: Entry count and embedding status
    local total embedded
    total=$(docker exec memlayer-db psql -U memlayer -d memlayer -tAc "SELECT COUNT(*) FROM memory_entries")
    embedded=$(docker exec memlayer-db psql -U memlayer -d memlayer -tAc "SELECT COUNT(*) FROM memory_entries WHERE embedding IS NOT NULL")
    info "Entries: $total total, $embedded with embeddings"

    # Check 3: Orphaned entries (no session)
    local orphaned
    orphaned=$(docker exec memlayer-db psql -U memlayer -d memlayer -tAc \
        "SELECT COUNT(*) FROM memory_entries me LEFT JOIN claude_sessions cs ON me.session_id = cs.session_id WHERE cs.session_id IS NULL")
    if [[ "$orphaned" != "0" ]]; then
        warn "Found $orphaned entries with no matching session"
        issues=$((issues + 1))
    else
        success "No orphaned entries"
    fi

    # Check 4: Duplicate payload hashes
    local dupes
    dupes=$(docker exec memlayer-db psql -U memlayer -d memlayer -tAc \
        "SELECT COUNT(*) FROM (SELECT payload_hash, COUNT(*) c FROM memory_entries GROUP BY payload_hash HAVING COUNT(*) > 1) sub")
    if [[ "$dupes" != "0" ]]; then
        warn "Found $dupes duplicate payload hashes"
        issues=$((issues + 1))
    else
        success "No duplicate entries"
    fi

    # Check 5: Response file consistency
    local missing_files
    missing_files=$(docker exec memlayer-db psql -U memlayer -d memlayer -tAc \
        "SELECT COUNT(*) FROM response_files WHERE deleted_at IS NULL")
    info "Response files: $missing_files active records"

    # Check 6: Migration status
    local migrations
    migrations=$(docker exec memlayer-db psql -U memlayer -d memlayer -tAc "SELECT COUNT(*) FROM applied_migrations")
    info "Migrations: $migrations applied"

    echo
    if [[ $issues -eq 0 ]]; then
        success "All integrity checks passed"
    else
        warn "$issues issue(s) found"
    fi
}

# ── Main dispatch ───────────────────────────────────────────────────
case "${1:-}" in
    setup)    shift; "$REPO_DIR/setup_${2:-server}.sh" "$@" ;;
    status)   shift; cmd_status "$@" ;;
    backup)   shift; cmd_backup "$@" ;;
    restore)  shift; cmd_restore "$@" ;;
    forget)   shift; cmd_forget "$@" ;;
    verify)   shift; cmd_verify "$@" ;;
    version)  echo "memlayer-server $VERSION" ;;
    -h|--help|"") usage ;;
    *)        error "Unknown command: $1"; usage; exit 1 ;;
esac
