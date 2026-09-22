/**
 * Content Hash Utilities
 * Computes SHA-256 hashes for file contents to enable deduplication and change detection
 */

import * as crypto from 'crypto';
import * as fs from 'fs';
import * as path from 'path';

export interface FileContent {
    filePath: string;
    contentHash: string;
    size: number;
}

/**
 * Compute SHA-256 hash of file contents
 */
export function computeFileHash(filePath: string): string {
    const buffer = fs.readFileSync(filePath);
    return crypto.createHash('sha256').update(buffer).digest('hex');
}

/**
 * Compute hash from content string (for in-memory comparison)
 */
export function computeStringHash(content: string): string {
    return crypto.createHash('sha256').update(content).digest('hex');
}

/**
 * Check if file has changed by comparing hashes
 */
export function hasFileChanged(filePath: string, previousHash?: string): boolean {
    if (!previousHash) {
        return true; // No previous hash means it's new
    }
    
    const currentHash = computeFileHash(filePath);
    return currentHash !== previousHash;
}

/**
 * Get file metadata including hash
 */
export function getFileMetadata(filePath: string): FileContent | null {
    try {
        if (!fs.existsSync(filePath)) {
            return null;
        }
        
        const stat = fs.statSync(filePath);
        const contentHash = computeFileHash(filePath);
        
        return {
            filePath,
            contentHash,
            size: stat.size
        };
    } catch (error) {
        console.error(`Error reading file ${filePath}:`, error);
        return null;
    }
}

/**
 * Batch compute hashes for multiple files
 */
export function batchComputeHashes(filePaths: string[]): FileContent[] {
    const results: FileContent[] = [];
    
    for (const filePath of filePaths) {
        const metadata = getFileMetadata(filePath);
        if (metadata) {
            results.push(metadata);
        }
    }
    
    return results;
}

/**
 * Find duplicate files by content hash
 */
export function findDuplicates(fileContents: FileContent[]): Map<string, FileContent[]> {
    const hashMap = new Map<string, FileContent[]>();
    
    for (const file of fileContents) {
        if (!hashMap.has(file.contentHash)) {
            hashMap.set(file.contentHash, []);
        }
        hashMap.get(file.contentHash)!.push(file);
    }
    
    // Filter to only groups with duplicates
    const duplicates = new Map<string, FileContent[]>();
    for (const [hash, files] of hashMap.entries()) {
        if (files.length > 1) {
            duplicates.set(hash, files);
        }
    }
    
    return duplicates;
}
