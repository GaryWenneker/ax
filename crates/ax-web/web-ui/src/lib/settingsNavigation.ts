/** Every entry points to an existing settings surface; no mock-only controls. */
export type SettingsSectionId = 'appearance' | 'agents' | 'command-center' | 'remote' | 'sharing' | 'replication' | 'vault' | 'export' | 'plugins' | 'embeddings' | 'policy-sharing';
export const SETTINGS_GROUPS: { label: string; items: { id: SettingsSectionId; label: string; icon: string }[] }[] = [
  { label: 'General', items: [
    { id: 'appearance', label: 'Appearance', icon: 'color-mode' },
    { id: 'command-center', label: 'Command Center', icon: 'settings-gear' },
  ] },
  { label: 'Connections', items: [
    { id: 'agents', label: 'IDEs & agents', icon: 'device-desktop' },
    { id: 'remote', label: 'Remote MCP', icon: 'remote' },
    { id: 'plugins', label: 'Plugins', icon: 'extensions' },
  ] },
  { label: 'Data', items: [
    { id: 'replication', label: 'Policy sync', icon: 'sync' },
    { id: 'sharing', label: 'Sharing', icon: 'broadcast' },
    { id: 'policy-sharing', label: 'Policy sharing', icon: 'organization' },
    { id: 'vault', label: 'Vault connection', icon: 'database' },
    { id: 'export', label: 'Knowledge export', icon: 'export' },
    { id: 'embeddings', label: 'Embeddings', icon: 'symbol-array' },
  ] },
];
