/** IANA timezone helpers for Command Center Logging timestamps. */

/** Empty / local / browser → use the browser's resolved zone. */
export function resolveTimeZone(configured?: string | null): string {
  const t = (configured ?? '').trim();
  if (!t || t === 'local' || t === 'browser') {
    try {
      return Intl.DateTimeFormat().resolvedOptions().timeZone || 'UTC';
    } catch {
      return 'UTC';
    }
  }
  return t;
}

export function browserTimeZone(): string {
  return resolveTimeZone('local');
}

/** Format an instant as `YYYY-MM-DD HH:MM:SS.mmm` in the given IANA zone. */
export function formatInstantInZone(
  ms: number,
  timeZone: string,
): { time: string; day: string } {
  const tz = resolveTimeZone(timeZone);
  const d = new Date(ms);
  if (Number.isNaN(d.getTime())) {
    return { time: '—', day: '' };
  }

  try {
    const parts = new Intl.DateTimeFormat('en-US', {
      timeZone: tz,
      year: 'numeric',
      month: '2-digit',
      day: '2-digit',
      hour: '2-digit',
      minute: '2-digit',
      second: '2-digit',
      hour12: false,
    }).formatToParts(d);

    const get = (type: Intl.DateTimeFormatPartTypes) =>
      parts.find((p) => p.type === type)?.value ?? '';

    let hour = get('hour');
    // Some engines emit "24" for midnight with hour12:false — normalize.
    if (hour === '24') hour = '00';

    const day = `${get('year')}-${get('month')}-${get('day')}`;
    const msPart = String(Math.floor(ms % 1000)).padStart(3, '0');
    const time = `${day} ${hour}:${get('minute')}:${get('second')}.${msPart}`;
    return { time, day };
  } catch {
    // Invalid IANA id — fall back to UTC ISO strip.
    const iso = d.toISOString();
    const time = iso.replace('T', ' ').replace(/Z$/, '');
    return { time, day: time.slice(0, 10) };
  }
}

/** Common IANA zones for the Settings picker (plus browser local). */
export const TIMEZONE_OPTIONS: { value: string; label: string }[] = [
  { value: '', label: 'Browser local' },
  { value: 'UTC', label: 'UTC' },
  { value: 'Europe/Amsterdam', label: 'Europe/Amsterdam' },
  { value: 'Europe/London', label: 'Europe/London' },
  { value: 'Europe/Berlin', label: 'Europe/Berlin' },
  { value: 'Europe/Paris', label: 'Europe/Paris' },
  { value: 'America/New_York', label: 'America/New_York' },
  { value: 'America/Chicago', label: 'America/Chicago' },
  { value: 'America/Denver', label: 'America/Denver' },
  { value: 'America/Los_Angeles', label: 'America/Los_Angeles' },
  { value: 'America/Sao_Paulo', label: 'America/Sao_Paulo' },
  { value: 'Asia/Dubai', label: 'Asia/Dubai' },
  { value: 'Asia/Kolkata', label: 'Asia/Kolkata' },
  { value: 'Asia/Singapore', label: 'Asia/Singapore' },
  { value: 'Asia/Tokyo', label: 'Asia/Tokyo' },
  { value: 'Asia/Shanghai', label: 'Asia/Shanghai' },
  { value: 'Australia/Sydney', label: 'Australia/Sydney' },
  { value: 'Pacific/Auckland', label: 'Pacific/Auckland' },
];
