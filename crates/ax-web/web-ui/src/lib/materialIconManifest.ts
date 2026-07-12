import manifest from 'material-icon-theme/dist/material-icons.json';

export interface IconDefinition {
  iconPath: string;
}

export interface MaterialIconManifest {
  iconDefinitions: Record<string, IconDefinition>;
  fileNames: Record<string, string>;
  fileExtensions: Record<string, string>;
  languageIds: Record<string, string>;
  folderNames: Record<string, string>;
  folderNamesExpanded: Record<string, string>;
  rootFolderNames: Record<string, string>;
  rootFolderNamesExpanded: Record<string, string>;
  file: string;
  folder: string;
  folderExpanded: string;
  rootFolder: string;
  rootFolderExpanded: string;
}

export const iconManifest = manifest as MaterialIconManifest;

const ICON_BASE = `${import.meta.env.BASE_URL}material-icons/`;

export function materialIconUrl(iconId: string): string {
  const def = iconManifest.iconDefinitions[iconId];
  const file = def?.iconPath?.split('/').pop() ?? 'file.svg';
  return `${ICON_BASE}${file}`;
}

function basename(name: string): string {
  const parts = name.replace(/\\/g, '/').split('/');
  return (parts[parts.length - 1] ?? name).toLowerCase();
}

function matchExtension(fileName: string): string | undefined {
  const lower = fileName.toLowerCase();
  let bestIcon: string | undefined;
  let bestLen = 0;

  for (const [ext, iconId] of Object.entries(iconManifest.fileExtensions)) {
    const suffix = ext.startsWith('.') ? ext.toLowerCase() : `.${ext.toLowerCase()}`;
    if (lower.endsWith(suffix) && suffix.length > bestLen) {
      bestIcon = iconId;
      bestLen = suffix.length;
    }
  }
  return bestIcon;
}

export function resolveFileIconId(fileName: string, language?: string): string {
  const base = basename(fileName);

  if (iconManifest.fileNames[base]) {
    return iconManifest.fileNames[base];
  }

  const fromExt = matchExtension(base);
  if (fromExt) return fromExt;

  if (language) {
    const lang = language.toLowerCase();
    if (iconManifest.languageIds[lang]) {
      return iconManifest.languageIds[lang];
    }
  }

  return iconManifest.file;
}

export function resolveFolderIconId(folderName: string, open: boolean, isRoot = false): string {
  const key = folderName.toLowerCase();

  if (isRoot) {
    if (open && iconManifest.rootFolderNamesExpanded[key]) {
      return iconManifest.rootFolderNamesExpanded[key];
    }
    if (!open && iconManifest.rootFolderNames[key]) {
      return iconManifest.rootFolderNames[key];
    }
    return open ? iconManifest.rootFolderExpanded : iconManifest.rootFolder;
  }

  if (open && iconManifest.folderNamesExpanded[key]) {
    return iconManifest.folderNamesExpanded[key];
  }
  if (!open && iconManifest.folderNames[key]) {
    return iconManifest.folderNames[key];
  }

  return open ? iconManifest.folderExpanded : iconManifest.folder;
}
