#!/usr/bin/env node
/**
 * Dry-Run Migration Tool
 * Analyzes project databases and prepares them for global sync
 * Does NOT actually migrate data, just reports findings
 */

const fs = require('fs');
const path = require('path');
const { execSync } = require('child_process');

// Database connection helper
function getDbPath(projectPath) {
    const axDb = path.join(projectPath, '.ax', 'ax.db');
    const legacy = path.join(projectPath, '.ax', 'project.db');
    if (fs.existsSync(axDb)) return axDb;
    return legacy;
}

function checkDatabase(dbPath) {
    console.log(`\n📁 Checking: ${dbPath}`);
    
    if (!fs.existsSync(dbPath)) {
        console.log('   ❌ Database not found!');
        return null;
    }
    
    const dbSize = (fs.statSync(dbPath).size / 1024 / 1024).toFixed(2);
    console.log(`   ✅ Found (${dbSize} MB)`);
    
    // Check tables
    try {
        const tables = execSync(
            `sqlite3 "${dbPath}" "SELECT name FROM sqlite_master WHERE type='table' ORDER BY name;"`,
            { encoding: 'utf8' }
        ).trim().split('\n');
        
        console.log(`   📋 Tables: ${tables.join(', ')}`);
        
        // Check for sync columns
        const hasSyncColumn = execSync(
            `sqlite3 "${dbPath}" "PRAGMA table_info(nodes);" | grep -c 'last_global_sync' || echo 0`,
            { encoding: 'utf8' }
        ).trim();
        
        if (hasSyncColumn === '0') {
            console.log('   ⚠️  Sync columns NOT present - ready for extension');
        } else {
            console.log('   ✅ Sync columns already present');
        }
        
        // Get node count
        const nodeCount = execSync(
            `sqlite3 "${dbPath}" "SELECT COUNT(*) FROM nodes;"`,
            { encoding: 'utf8' }
        ).trim();
        
        console.log(`   📊 Nodes: ${nodeCount}`);
        
        return {
            path: dbPath,
            size: dbSize,
            tables,
            hasSyncColumn: hasSyncColumn !== '0',
            nodeCount: parseInt(nodeCount)
        };
    } catch (error) {
        console.log('   ❌ Error analyzing database');
        return null;
    }
}

// Main function
function main() {
    const args = process.argv.slice(2);
    
    if (args.length === 0) {
        console.error('Usage: node dry-run-migration.js <project_path>');
        console.error('Example: node dry-run-migration.js /Users/gary/io/ax/project_a');
        process.exit(1);
    }
    
    const projectPath = args[0];
    
    if (!fs.existsSync(projectPath)) {
        console.error(`❌ Project path not found: ${projectPath}`);
        process.exit(1);
    }
    
    console.log('🔍 DRY-RUN MIGRATION ANALYSIS');
    console.log('=' .repeat(50));
    console.log(`\nAnalyzing project: ${path.basename(projectPath)}`);
    
    const dbPath = getDbPath(projectPath);
    const result = checkDatabase(dbPath);
    
    if (result) {
        console.log('\n✅ DRY-RUN COMPLETE');
        console.log('=' .repeat(50));
        console.log(`\nMigration readiness: ${result.hasSyncColumn ? 'READY' : 'NEEDS EXTENSION'}`);
        console.log(`Next step: Run project-extension.schema.sql on this database`);
    } else {
        console.log('\n⚠️  Could not analyze database');
    }
}

main();
