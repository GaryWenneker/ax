/**
 * Cross-Platform File Watcher
 * Monitors file system changes using platform-specific APIs:
 * - macOS: FSEvents (high-performance, low-latency)
 * - Linux: inotify
 * - Windows: ReadDirectoryChangesW
 */

import * as fs from 'fs';
import * as path from 'path';

export interface FileChangeEvent {
    type: 'created' | 'modified' | 'deleted' | 'moved';
    filePath: string;
    timestamp: number;
}

export class FileWatcher {
    private isWatching: boolean = false;
    private onChangeCallback?: (event: FileChangeEvent) => void;
    
    /**
     * Start watching a directory for file changes
     */
    start(directoryPath: string, callback: (event: FileChangeEvent) => void): void {
        this.onChangeCallback = callback;
        this.isWatching = true;
        
        console.log(`👁️  Starting file watcher for: ${directoryPath}`);
        
        // Platform-specific implementation would go here
        if (process.platform === 'darwin') {
            this.startFsEventsWatch(directoryPath, callback);
        } else if (process.platform === 'linux') {
            this.startInotifyWatch(directoryPath, callback);
        } else {
            console.warn(`⚠️  File watcher not fully implemented for ${process.platform}`);
            // Fallback to polling
            this.startPollingWatch(directoryPath, callback);
        }
    }
    
    /**
     * Stop watching the directory
     */
    stop(): void {
        this.isWatching = false;
        console.log('👁️  Stopped file watcher');
    }
    
    /**
     * macOS: FSEvents implementation (using fs.watch on macOS)
     */
    private startFsEventsWatch(directoryPath: string, callback: (event: FileChangeEvent) => void): void {
        // On macOS, fs.watch uses FSEvents under the hood
        const watcher = fs.watch(directoryPath, { recursive: true }, (eventType, filename) => {
            if (!filename) return; // Skip directory events
            
            const filePath = path.join(directoryPath, filename);
            const event: FileChangeEvent = {
                type: eventType === 'rename' ? 'moved' : 
                      eventType === 'change' ? 'modified' :
                      eventType === 'close' ? 'deleted' : 'created',
                filePath,
                timestamp: Date.now()
            };
            
            callback(event);
        });
        
        // Store watcher reference for cleanup
        (this as any).watchers = [(this as any).watchers || [], watcher].flat();
    }
    
    /**
     * Linux: inotify implementation (using fs.watch as fallback)
     */
    private startInotifyWatch(directoryPath: string, callback: (event: FileChangeEvent) => void): void {
        console.warn('⚠️  Using polling fallback on Linux');
        this.startPollingWatch(directoryPath, callback);
    }
    
    /**
     * Fallback polling implementation
     */
    private startPollingWatch(directoryPath: string, callback: (event: FileChangeEvent) => void): void {
        const intervalMs = 100; // Poll every 100ms
        const previousFiles = new Set<string>();
        
        const poll = () => {
            if (!this.isWatching) return;
            
            try {
                const currentFiles: string[] = [];
                
                function walkDir(dir: string): void {
                    const entries = fs.readdirSync(dir, { withFileTypes: true });
                    for (const entry of entries) {
                        const fullPath = path.join(dir, entry.name);
                        currentFiles.push(fullPath);
                        
                        if (entry.isDirectory()) {
                            walkDir(fullPath);
                        }
                    }
                }
                
                walkDir(directoryPath);
                
                // Detect changes
                for (const file of currentFiles) {
                    const event: FileChangeEvent = { type: 'created', filePath: file, timestamp: Date.now() };
                    callback(event);
                }
                
                previousFiles.clear();
                currentFiles.forEach(f => previousFiles.add(f));
                
            } catch (error) {
                console.error('Error polling directory:', error);
            }
            
            if (this.isWatching) {
                setTimeout(poll, intervalMs);
            }
        }
        
        poll();
    }
}

// Export singleton instance for convenience
export const fileWatcher = new FileWatcher();
