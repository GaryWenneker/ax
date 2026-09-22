/**
 * Sync Strategy Implementation
 * Determines whether to perform incremental or full sync based on content hashes
 */

export type SyncType = 'incremental' | 'full';

export interface SyncOperation {
    type: SyncType;
    sourceProject: string;
    targetDatabase: 'global';
    records: Array<{
        id: string;
        contentHash: string;
        timestamp: number;
        changed?: boolean; // true if existing record was updated
    }>;
}

export class SyncStrategy {
    /**
     * Determine sync strategy based on local vs global hash comparison
     */
    async determineSyncType(
        localHashes: Map<string, { filePath: string; changed?: boolean }>,
        globalHashes: Set<string>
    ): Promise<SyncType> {
        const newRecords = Array.from(localHashes.entries()).filter(
            ([hash]) => !globalHashes.has(hash)
        ).length;
        
        const updatedRecords = Array.from(localHashes.entries()).filter(
            ([hash]) => globalHashes.has(hash)
        ).length;
        
        // If all records are new or no existing records, do full sync
        if (newRecords === localHashes.size || globalHashes.size === 0) {
            return 'full';
        }
        
        // Otherwise incremental sync
        return 'incremental';
    }
    
    /**
     * Check if full rebuild is needed (e.g., after major schema changes)
     */
    async needsFullRebuild(sourceProject: string): Promise<boolean> {
        const metadata = await this.getProjectMetadata(sourceProject);
        return !metadata?.lastFullSync || 
               Date.now() - new Date(metadata.lastFullSync).getTime() > 7 * 24 * 60 * 60 * 1000; // Weekly
    }
    
    /**
     * Get project metadata from database
     */
    private async getProjectMetadata(projectName: string): Promise<{
        lastFullSync?: Date;
    } | null> {
        // Implementation would query project_metadata table
        return null; // Placeholder
    }
}
