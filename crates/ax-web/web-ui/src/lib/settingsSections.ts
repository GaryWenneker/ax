export type SettingsSection = {
  id: string;
  label: string;
};

export type SettingsSectionGroup = {
  id: string;
  label: string;
  sections: SettingsSection[];
};

export const SETTINGS_SECTION_GROUPS: SettingsSectionGroup[] = [
  {
    id: 'integrations',
    label: 'Integrations',
    sections: [
      { id: 'settings-ide-agents', label: 'IDEs & agents' },
      { id: 'settings-sharing', label: 'Sharing' },
      { id: 'settings-plugins', label: 'Plugins' },
    ],
  },
  {
    id: 'workspace',
    label: 'Workspace',
    sections: [
      { id: 'settings-vault', label: 'Vault connection' },
      { id: 'settings-okf', label: 'Open Knowledge Format' },
    ],
  },
  {
    id: 'intelligence',
    label: 'Intelligence',
    sections: [
      { id: 'settings-embeddings', label: 'Embeddings' },
      { id: 'settings-policy-sharing', label: 'Policy sharing' },
    ],
  },
  {
    id: 'command-center',
    label: 'Command Center',
    sections: [{ id: 'settings-command-center', label: 'Command Center' }],
  },
];

export const SETTINGS_SECTIONS = SETTINGS_SECTION_GROUPS.flatMap((group) => group.sections);

export function isSettingsSectionId(value: string): boolean {
  return SETTINGS_SECTIONS.some((section) => section.id === value);
}

export function settingsSectionLabel(id: string): string {
  return SETTINGS_SECTIONS.find((section) => section.id === id)?.label ?? 'Settings';
}
