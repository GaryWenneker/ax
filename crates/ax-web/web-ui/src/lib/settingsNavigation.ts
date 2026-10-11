/** Every entry points to an existing settings surface; no mock-only controls. */
export type SettingsSectionId = 'team' | 'clients' | 'nodes' | 'appearance' | 'agents' | 'command-center' | 'remote' | 'sharing' | 'replication' | 'vault' | 'export' | 'plugins' | 'embeddings' | 'policy-sharing';
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
  { label: 'Access', items: [
    { id: 'team', label: 'Team', icon: 'organization' },
    { id: 'clients', label: 'Clients & tokens', icon: 'key' },
    { id: 'nodes', label: 'Replication', icon: 'server' },
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

export function readSettingsSection(): SettingsSectionId {
  const requested = new URLSearchParams(window.location.search).get('section');
  return SETTINGS_GROUPS.flatMap(group => group.items).find(item => item.id === requested)?.id ?? 'appearance';
}
