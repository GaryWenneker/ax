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

export interface ProjectSyncMetadata {
    lastFullSync?: Date;
}

export type ProjectMetadataLoader = (
    projectName: string,
) => Promise<ProjectSyncMetadata | null>;

export class SyncStrategy {
    private readonly loadProjectMetadata?: ProjectMetadataLoader;

    constructor(loadProjectMetadata?: ProjectMetadataLoader) {
        this.loadProjectMetadata = loadProjectMetadata;
    }

    /**
     * Determine sync strategy based on local vs global hash comparison
     */
    async determineSyncType(
        localHashes: Map<string, { filePath: string; changed?: boolean }>,
        globalHashes: Set<string>,
    ): Promise<SyncType> {
        if (globalHashes.size === 0) {
            return 'full';
        }

        const newRecords = Array.from(localHashes.entries()).filter(
            ([hash]) => !globalHashes.has(hash),
        ).length;

        // All hashes in this batch are new — full only when the batch is the whole tree.
        if (newRecords === localHashes.size && localHashes.size > 1) {
            return 'full';
        }

        return 'incremental';
    }

    /**
     * Check if full rebuild is needed (e.g., after major schema changes)
     */
    async needsFullRebuild(sourceProject: string): Promise<boolean> {
        const metadata = await this.getProjectMetadata(sourceProject);
        if (!metadata?.lastFullSync) {
            return true;
        }
        const ageMs =
            Date.now() - new Date(metadata.lastFullSync).getTime();
        return ageMs > 7 * 24 * 60 * 60 * 1000; // Weekly
    }

    /**
     * Get project metadata from database
     */
    private async getProjectMetadata(
        projectName: string,
    ): Promise<ProjectSyncMetadata | null> {
        if (this.loadProjectMetadata) {
            return this.loadProjectMetadata(projectName);
        }
        return null;
    }
}