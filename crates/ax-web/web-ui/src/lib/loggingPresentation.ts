import type {TraceEntry} from './mcpTrace';

const DOMAIN_ACTIONS: Partial<Record<TraceEntry['kind'], {pattern: RegExp; start: string; ok: string}>> = {
  workspace: {pattern: /^workspace sync (start|ok)(?:\s|$)/, start: 'Workspace sync started', ok: 'Workspace sync completed'},
  memory: {pattern: /^memory capture-git (start|ok)(?:\s|$)/, start: 'Git history capture started', ok: 'Git history captured'},
  ship: {pattern: /^ship(?:-ci| ci)? (start|ok)(?:\s|$)/, start: 'Quality check started', ok: 'Quality check completed'},
};

function withoutLoggerPrefix(text: string): string {
  return text.replace(/^(?:\[(?:ax|ax-mcp)\]\s*)?(?:🪓\s*)?/, '');
}

function domainFacts(message: string): string {
  const fields = new Map([...message.matchAll(/(?:^|\s)([a-z_]+)=([^\s]+)/g)].map(match => [match[1], match[2]]));
  const facts: string[] = [];
  for (const [key, label] of [['files','files'],['scanned','scanned'],['captured','captured'],['duration_ms','ms'],['limit','limit']]) {
    const value=fields.get(key);
    if (value && /^\d+$/.test(value)) facts.push(key==='limit'?`Limit ${value}`:`${value} ${label}`);
  }
  const mode=fields.get('mode');
  if (mode) facts.push(mode==='ci'?'CI':mode==='evaluate'?'Evaluate':mode);
  const passed=fields.get('passed');
  if (passed==='1' || passed==='0') facts.push(passed==='1'?'Passed':'Failed');
  const watch=fields.get('watch');
  if (watch==='1' || watch==='0') facts.push(watch==='1'?'Watch on':'Watch off');
  return facts.join(' · ');
}

/** Presentation only: raw entries remain available for search and inspection. */
export function loggingRowPresentation(entry: Readonly<Pick<TraceEntry, 'kind' | 'message' | 'tool'>>, {title, subtitle, text}: {title: string; subtitle: string; text?: string}): {title: string; subtitle: string} {
  if (text) return {title: text, subtitle};
  const action=DOMAIN_ACTIONS[entry.kind];
  const phase=action?.pattern.exec(withoutLoggerPrefix(entry.message))?.[1];
  if (action && (phase==='start' || phase==='ok')) return {title: action[phase], subtitle: domainFacts(entry.message)};
  return {title: withoutLoggerPrefix(title), subtitle};
}
