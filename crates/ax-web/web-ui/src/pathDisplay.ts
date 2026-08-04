/** Strip Windows `\\?\` extended-length prefix for display. */
export function displayPath(path: string): string {
  let cleaned = path.replace(/^\\\\\?\\/, '');
  // Keep UNC as backslashes; otherwise prefer platform-looking separators from input.
  if (!cleaned.startsWith('\\\\') && cleaned.includes('/') && !cleaned.includes('\\')) {
    return cleaned.replace(/\/+$/, '') || '/';
  }
  cleaned = cleaned.replace(/\//g, '\\');
  return cleaned.replace(/\\+$/, '') || cleaned;
}

export interface PathCrumb {
  label: string;
  path: string;
}

/** Split a filesystem path into clickable breadcrumb segments. */
export function pathBreadcrumbs(path: string): PathCrumb[] {
  const raw = path.replace(/^\\\\\?\\/, '');
  if (!raw) {
    return [];
  }

  const isUnix = raw.startsWith('/') && !raw.startsWith('//');
  const unc = raw.startsWith('\\\\') || raw.startsWith('//');
  const sep = isUnix ? '/' : '\\';
  const parts = raw.split(/[\\/]+/).filter(Boolean);
  if (!parts.length) {
    return [{ label: isUnix ? '/' : raw, path: isUnix ? '/' : raw }];
  }

  const crumbs: PathCrumb[] = [];
  let acc = '';

  if (unc) {
    if (parts.length >= 2) {
      acc = `\\\\${parts[0]}\\${parts[1]}`;
      crumbs.push({ label: `\\\\${parts[0]}\\${parts[1]}`, path: acc });
      for (let i = 2; i < parts.length; i++) {
        acc = `${acc}\\${parts[i]}`;
        crumbs.push({ label: parts[i], path: acc });
      }
      return crumbs;
    }
  }

  for (let i = 0; i < parts.length; i++) {
    const part = parts[i];
    if (i === 0 && /^[A-Za-z]:$/.test(part)) {
      acc = `${part}\\`;
      crumbs.push({ label: part, path: acc });
      continue;
    }
    if (i === 0 && isUnix) {
      acc = `/${part}`;
      crumbs.push({ label: part, path: acc });
      continue;
    }
    if (i === 0) {
      acc = part;
      crumbs.push({ label: part, path: acc });
      continue;
    }
    acc = acc.endsWith(sep) ? `${acc}${part}` : `${acc}${sep}${part}`;
    crumbs.push({ label: part, path: acc });
  }

  return crumbs;
}
