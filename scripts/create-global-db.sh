#!/bin/bash
# Script to initialize the global multi-tenant database
# Location: ~/.ax/global.db

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="${SCRIPT_DIR}/.."
GLOBAL_DB_PATH="$HOME/.ax/global.db"

echo "🌍 Initializing Global Multi-Tenant Database..."
echo "   Path: $GLOBAL_DB_PATH"

# Never write into an existing database: it may be the live one.
if [ -e "$GLOBAL_DB_PATH" ]; then
    echo "❌ $GLOBAL_DB_PATH already exists; move it aside first if you want a fresh one." >&2
    exit 1
fi

# Build the database in a temporary file next to the target, then move it into place.
mkdir -p "$(dirname "$GLOBAL_DB_PATH")"
TMP_DB="$(mktemp "$GLOBAL_DB_PATH.XXXXXX")"
trap 'rm -f "$TMP_DB"' EXIT
rm -f "$TMP_DB"
sqlite3 "$TMP_DB" < "$PROJECT_ROOT/database/schema/global.db.schema.sql"
if ! mv -n "$TMP_DB" "$GLOBAL_DB_PATH" || [ -e "$TMP_DB" ]; then
    echo "❌ $GLOBAL_DB_PATH appeared while the script ran; left it unchanged." >&2
    exit 1
fi

# Verify creation
if [ -f "$GLOBAL_DB_PATH" ]; then
    DB_SIZE=$(du -h "$GLOBAL_DB_PATH" | cut -f1)
    echo "✅ Global database created successfully!"
    echo "   Size: $DB_SIZE"
    
    # Show table count
    TABLE_COUNT=$(sqlite3 "$GLOBAL_DB_PATH" "SELECT COUNT(*) FROM sqlite_master WHERE type='table';")
    echo "   Tables: $TABLE_COUNT"
else
    echo "❌ Failed to create global database!"
    exit 1
fi

echo ""
echo "📋 Next steps:"
echo "   1. ax global init && ax global sync"
echo "   2. Dry-run: node scripts/dry-run-migration.js /path/to/project"