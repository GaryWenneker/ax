import {extractFields, extractPayloadJson, tryParseJson, type TraceEntry} from './mcpTrace';

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

const TOOL_LABELS: Record<string, string> = {
  ax_preflight: 'Preflight',
  ax_node: 'Node',
  ax_explore: 'Explore',
  ax_guard: 'Check',
  ax_search: 'Search',
  workspace: 'Sync',
  ship: 'Quality',
  'ship-ci': 'Quality',
  ax_skill: 'Skill',
  memory: 'Memory',
  ax_sync: 'Index',
  ax_callers: 'Callers',
  ax_rules: 'Rules',
  ax_policy_capture: 'Capture',
  ax_status: 'Status',
  policy: 'Policy',
  ax_session: 'Note',
  ax_remember: 'Remember',
  ax_impact: 'Impact',
  ax_recall: 'Recall',
  ax_affected: 'Affected',
  ax_history: 'History',
  'cli:status': 'CLI status',
  lsp: 'Language server',
  action: 'Step',
  ax_path: 'Path',
  'cli:explore': 'CLI explore',
  'cli:impact': 'CLI impact',
  ax_index: 'Rebuild',
  ax_report: 'Report',
  ax_callees: 'Uses',
  ax_expand: 'Expand',
  'cli:validate': 'CLI validate',
  'cli:path': 'CLI path',
  'cli:cycles': 'CLI cycles',
  'cli:api': 'CLI API',
};

const KIND_LABELS: Record<string, string> = {
  enrich: 'Enrich',
  inbound: 'Inbound',
  outbound: 'Outbound',
  preview: 'Preview',
  error: 'Error',
  internal: 'Internal',
  plugin: 'Plugin',
  lsp: 'Language server',
  ship: 'Quality',
  share: 'Share',
  workspace: 'Sync',
  embed: 'Embed',
  action: 'Step',
  memory: 'Memory',
  policy: 'Policy',
  cli: 'CLI',
};

export type LoggingRowView = {
  label: string;
  title: string;
  sessionId: string | null;
  sessionShort: string | null;
};

type RowEntry = Readonly<Pick<TraceEntry, 'kind' | 'message' | 'tool'>>;

/** Human label for a tool id. Underscores and the ax_ prefix stay out of the row. */
export function loggingToolLabel(tool: string | null | undefined, kind?: string): string {
  if (tool && TOOL_LABELS[tool]) return TOOL_LABELS[tool];
  if (tool) {
    const cli = tool.startsWith('cli:') ? `CLI ${tool.slice(4)}` : tool.replace(/^ax_/, '');
    return cli
      .split(/[_:-]+/)
      .filter(Boolean)
      .map((word) => word.charAt(0).toUpperCase() + word.slice(1))
      .join(' ');
  }
  return (kind && KIND_LABELS[kind]) || 'Event';
}

function fieldsOf(message: string): Map<string, string> {
  return new Map(extractFields(withoutLoggerPrefix(message)).map((field) => [field.key, field.value]));
}

function argsOf(entry: RowEntry): Record<string, unknown> | null {
  const payload = extractPayloadJson(entry as TraceEntry);
  if (payload && typeof payload === 'object' && !Array.isArray(payload)) {
    return payload as Record<string, unknown>;
  }
  const raw = fieldsOf(entry.message).get('args');
  const parsed = raw ? tryParseJson(raw) : null;
  if (parsed && typeof parsed === 'object' && !Array.isArray(parsed)) {
    return parsed as Record<string, unknown>;
  }
  return null;
}

function textOf(value: unknown): string | null {
  return typeof value === 'string' && value.trim() ? value.trim() : null;
}

function digits(fields: Map<string, string>, key: string): string | null {
  const value = fields.get(key);
  return value && /^\d+$/.test(value) ? value : null;
}

function oneLine(value: string): string {
  const normalized = value.replace(/↵/g, '\n').replace(/⏎/g, '\n');
  const line =
    normalized
      .split('\n')
      .map((part) => part.trim())
      .find((part) => part && part !== '---' && !part.startsWith('```')) ?? '';
  const plain = line
    .replace(/^#{1,6}\s+/, '')
    .replace(/^<ax_[^>]*>\s*/, '')
    .replace(/\*\*/g, '')
    .replace(/`/g, '')
    .replace(/\bsession=\S+/g, '')
    .replace(/\s+/g, ' ')
    .trim();
  return plain.length > 160 ? `${plain.slice(0, 159)}…` : plain;
}

function shortSymbol(value: string): string {
  const leaf = value.split('::').pop() ?? value;
  return leaf || value;
}

function baseName(value: string): string {
  const parts = value.split(/[/\\]/);
  return parts[parts.length - 1] || value;
}

function shortSession(id: string): string {
  if (/^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i.test(id)) {
    return id.slice(0, 8);
  }
  return id.replace(/^axs_/, '').slice(0, 8);
}

/** Session id written on the log line or inside the tool arguments. */
export function traceSessionId(entry: RowEntry): string | null {
  const fromArgs = textOf(argsOf(entry)?.session);
  if (fromArgs) return fromArgs;
  return fieldsOf(entry.message).get('session') ?? null;
}

function enrichTitle(fields: Map<string, string>, body: string): string {
  const rules = digits(fields, 'matched_rules');
  const skills = digits(fields, 'matched_skills');
  if (rules && skills) return `Policy matched · ${rules} rules · ${skills} skills`;
  const pending = digits(fields, 'pending_files');
  if (body.startsWith('index ') && pending) return `Index · ${pending} pending`;
  if (body.startsWith('directive none')) return 'Directive · none';
  if (body.startsWith('directive ')) return 'Directive · capture';
  const chars = digits(fields, 'final_inject_chars');
  if (body.startsWith('done ') && chars) return `Enrich done · ${chars} chars`;
  const turns = digits(fields, 'count');
  if (body.startsWith('turn history') && turns) return `Turn history · ${turns}`;
  if (body.startsWith('links ')) return 'Links added';
  if (body.startsWith('memories none')) return 'Memories · none';
  if (body.startsWith('memories ') && turns) return `Memories · ${turns}`;
  return 'Enrich';
}

function inboundTitle(tool: string | null, args: Record<string, unknown> | null, label: string): string {
  if (!args) return label;
  const prompt = textOf(args.prompt);
  if (prompt) return oneLine(prompt);
  const query = textOf(args.query);
  if (query) return oneLine(query);
  const symbol = textOf(args.name) ?? textOf(args.symbol);
  if (symbol && tool !== 'ax_skill') return shortSymbol(symbol);
  const operation = textOf(args.operation);
  const path = textOf(args.path);
  if (operation) {
    const verb = operation.charAt(0).toUpperCase() + operation.slice(1);
    return path ? `${verb} ${baseName(path)}` : verb;
  }
  if (tool === 'ax_skill') return textOf(args.name) ?? 'Skill';
  if (tool === 'ax_session') {
    const decisions = args.decisions;
    if (Array.isArray(decisions) && typeof decisions[0] === 'string') return oneLine(decisions[0]);
    return textOf(args.objective) ? oneLine(textOf(args.objective) as string) : 'Note';
  }
  if (tool === 'ax_remember') return oneLine(textOf(args.body) ?? 'Remember');
  if (tool === 'ax_recall') return textOf(args.id) ?? 'Recall';
  const from = textOf(args.from);
  const to = textOf(args.to);
  if (from && to) return `${shortSymbol(from)} → ${shortSymbol(to)}`;
  const files = args.files;
  if (Array.isArray(files) && typeof files[0] === 'string') return baseName(files[0]);
  if (tool === 'ax_expand') return 'Rest of the answer';
  if (tool === 'ax_index') return 'Full rebuild';
  if (tool === 'ax_status') return 'Index status';
  if (tool === 'ax_report') return 'Architecture report';
  if (tool === 'ax_sync') return 'Index update';
  if (tool === 'ax_rules') return 'Matching rules';
  if (tool === 'ax_policy_capture') return 'Save rule';
  return label;
}

function previewTitle(fields: Map<string, string>, label: string): string {
  const text = fields.get('text');
  if (!text) return 'Returned';
  if (text.trim().startsWith('{') || text.trim().startsWith('[')) {
    const parsed = tryParseJson(text);
    if (parsed && typeof parsed === 'object' && !Array.isArray(parsed)) {
      const record = parsed as Record<string, unknown>;
      if (Array.isArray(record.affected) && record.affected.length === 0) return 'No affected tests';
      const markdown = textOf(record.markdown);
      if (markdown) return oneLine(markdown);
    }
    return 'Returned';
  }
  const line = oneLine(text);
  const node = line.match(/^Node:\s+(.+)$/);
  if (node) return shortSymbol(node[1]);
  const explore = line.match(/^Explore:\s+(.+)$/);
  if (explore) return explore[1];
  const search = line.match(/^Search:\s*(.*)$/);
  if (search) return search[1] || 'Search';
  const callers = line.match(/^Callers of '([^']+)'/);
  if (callers) return shortSymbol(callers[1]);
  return line || label;
}

function domainTitle(kind: string, fields: Map<string, string>, body: string, label: string): string {
  if (kind === 'workspace' || body.startsWith('workspace sync')) {
    const files = digits(fields, 'files');
    if (body.includes(' sync ok') && files) return Number(files) === 1 ? '1 file synced' : `${files} files synced`;
    if (body.includes(' sync start')) return 'Sync started';
  }
  if (kind === 'ship' || body.startsWith('ship')) {
    if (fields.get('passed') === '0') return 'Check failed';
    if (body.includes(' ok')) return 'Check passed';
    if (body.includes(' start')) return 'Check started';
  }
  if (kind === 'memory' || body.startsWith('memory ')) {
    const captured = digits(fields, 'captured');
    if (body.includes(' ok') && captured) return `${captured} commits captured`;
    if (body.includes(' start')) return 'Capture started';
  }
  if (kind === 'policy' || body.startsWith('policy ')) {
    const rules = digits(fields, 'rules');
    const skills = digits(fields, 'skills');
    if (rules && skills) return `${rules} rules, ${skills} skills`;
    if (body.includes(' start')) return 'Policy refresh';
  }
  if (kind === 'lsp' || body.startsWith('lsp ')) {
    const resolved = digits(fields, 'resolved');
    if (resolved) return `${resolved} resolved`;
    return 'Language server';
  }
  if (kind === 'action') {
    const message = fields.get('message') ?? '';
    const resolved = message.match(/\bresolved=(\d+)/)?.[1];
    if (resolved) return `${resolved} resolved`;
    if (message.includes('start')) return 'Language server started';
    return oneLine(message) || 'Step';
  }
  if (kind === 'cli' || body.startsWith('cli ')) {
    if (fields.get('ok') === 'false') return 'Check failed';
    const count = digits(fields, 'count');
    if (count && fields.get('cmd') === 'cycles') return `${count} cycles`;
    if (count) return `${count} results`;
    if (body.includes(' start')) return 'Started';
    if (body.includes(' ok')) return 'Completed';
  }
  return label;
}

function rowTitle(entry: RowEntry, args: Record<string, unknown> | null, label: string): string {
  const fields = fieldsOf(entry.message);
  const body = withoutLoggerPrefix(entry.message).replace(/\s*session=\S+/g, '').trim();
  if (entry.kind === 'enrich' || body.startsWith('enrich ')) {
    return enrichTitle(fields, body.replace(/^enrich\s+/, ''));
  }
  if (entry.kind === 'inbound') return inboundTitle(entry.tool, args, label);
  if (entry.kind === 'preview') return previewTitle(fields, label);
  if (entry.kind === 'outbound') {
    const duration = digits(fields, 'duration_ms');
    return duration ? `${duration} ms` : 'Returned';
  }
  if (entry.kind === 'internal') {
    const chars = digits(fields, 'text_chars');
    return chars ? `${chars} chars` : 'Result';
  }
  if (entry.kind === 'error') {
    const message = fields.get('message');
    return message ? oneLine(message) : 'Error';
  }
  return domainTitle(entry.kind, fields, body, label);
}

/** Display-time row. The log line stays the source for search and the detail view. */
export function loggingRowView(entry: RowEntry): LoggingRowView {
  const args = argsOf(entry);
  const sessionId = textOf(args?.session) ?? fieldsOf(entry.message).get('session') ?? null;
  const label = loggingToolLabel(entry.tool, entry.kind);
  const fact = rowTitle(entry, args, label) || label;
  const title = fact === label || fact.toLowerCase().startsWith(`${label.toLowerCase()} `) ? fact : `${label} · ${fact}`;
  return {
    label,
    title,
    sessionId,
    sessionShort: sessionId ? shortSession(sessionId) : null,
  };
}
