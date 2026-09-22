#!/bin/bash
# Script to initialize the global multi-tenant database
# Location: ~/.ax/global.db

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="${SCRIPT_DIR}/.."
GLOBAL_DB_PATH="$HOME/.ax/global.db"

echo "🌍 Initializing Global Multi-Tenant Database..."
echo "   Path: $GLOBAL_DB_PATH"

# Create global database from schema
sqlite3 "$GLOBAL_DB_PATH" < "$PROJECT_ROOT/database/schema/global.db.schema.sql"

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