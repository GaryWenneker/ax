-- ============================================
-- DRY-RUN MIGRATION SCRIPT
-- Analyzes project databases and prepares for migration
-- Does NOT actually migrate data, just reports findings
-- ============================================

-- This script should be run via Node.js with proper database connections
-- Usage: node scripts/dry-run-migration.js <project_path>

SELECT 
    'DRY-RUN MIGRATION REPORT' as status,
    'Analyzing project structure for global sync compatibility...' as message;

-- Check if nodes table exists and has required columns
SELECT 
    'nodes_table_check' as check_type,
    CASE WHEN COUNT(*) > 0 THEN 'PASS' ELSE 'FAIL' END as result,
    GROUP_CONCAT(column_name) as columns_found
FROM pragma_table_info('nodes');

-- Check if documents table exists  
SELECT 
    'documents_table_check' as check_type,
    CASE WHEN COUNT(*) > 0 THEN 'PASS' ELSE 'FAIL' END as result,
    GROUP_CONCAT(column_name) as columns_found
FROM pragma_table_info('documents');

-- Report on sync column presence (should be NULL after extension)
SELECT 
    'sync_columns_check' as check_type,
    CASE WHEN COUNT(*) = 0 THEN 'PASS - Columns not yet added' ELSE 'ALREADY EXTENDED' END as result
FROM pragma_table_info('nodes') WHERE name IN ('last_global_sync');
