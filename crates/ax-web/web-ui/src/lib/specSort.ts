export type SpecSort = 'newest' | 'oldest' | 'title';

type Sortable = {
  id: string;
  title: string;
  updatedAt?: number | null;
};

export function specUpdatedAt(item: { updatedAt?: number | null }): number {
  const value = item.updatedAt;
  return typeof value === 'number' && Number.isFinite(value) ? value : 0;
}

export function sortSpecs<T extends Sortable>(items: readonly T[], sort: SpecSort): T[] {
  return [...items].sort((a, b) => {
    const byTitle = a.title.localeCompare(b.title, 'en', { sensitivity: 'base' });
    const byId = a.id.localeCompare(b.id);
    const byTime = specUpdatedAt(a) - specUpdatedAt(b);
    if (sort === 'title') return byTitle || -byTime || byId;
    if (sort === 'oldest') return byTime || byTitle || byId;
    return -byTime || byTitle || byId;
  });
}

export function specVersionLabel(item: {
  latestVersion?: number | null;
  latestStatus?: string | null;
  latestSource?: string | null;
  latestBasedOn?: number | null;
}): string {
  const version = item.latestVersion;
  if (typeof version !== 'number' || version <= 0) return '';
  const parts = [`v${version}`];
  if (item.latestStatus && item.latestStatus !== 'none') parts.push(item.latestStatus);
  if (item.latestSource) parts.push(item.latestSource);
  if (typeof item.latestBasedOn === 'number') parts.push(`based on v${item.latestBasedOn}`);
  return parts.join(' · ');
}

export function formatSpecDate(ms: number | null | undefined, timeZone?: string): string {
  if (typeof ms !== 'number' || !Number.isFinite(ms) || ms <= 0) return '';
  const parts = new Intl.DateTimeFormat('en-GB', {
    day: 'numeric',
    month: 'short',
    year: 'numeric',
    hour: '2-digit',
    minute: '2-digit',
    hourCycle: 'h23',
    timeZone,
  }).formatToParts(new Date(ms));
  const part = (type: Intl.DateTimeFormatPartTypes) => parts.find((item) => item.type === type)?.value ?? '';
  const day = part('day');
  const month = part('month');
  const year = part('year');
  const hour = part('hour');
  const minute = part('minute');
  if (!day || !month || !year || !hour || !minute) return '';
  return `${day} ${month} ${year}, ${hour}:${minute}`;
}
