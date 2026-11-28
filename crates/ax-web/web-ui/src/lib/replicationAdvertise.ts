/**
 * Split a saved advertised address into the form fields it came from: a plain
 * HTTP LAN address fills the IP picker (`ip`), anything else is a public proxy
 * URL. A malformed saved value is kept as text and never throws, so one bad
 * value cannot stop the replication status from loading.
 */
export function savedAdvertise(advertise: string | null | undefined): { ip: string | null; advertise: string } {
  if (!advertise) return { ip: null, advertise: '' };
  let endpoint: URL;
  try {
    endpoint = new URL(advertise);
  } catch {
    return { ip: null, advertise };
  }
  if (endpoint.protocol === 'http:') return { ip: endpoint.hostname.replace(/^\[|\]$/g, ''), advertise: '' };
  return { ip: null, advertise };
}
