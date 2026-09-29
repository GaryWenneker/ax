export interface MemoryBodyParts {
  prompt: string;
  filesNote: string | null;
  commits: string | null;
  changes: string | null;
  request: string | null;
  conversation: string | null;
  outcome: string | null;
}

export type FileChange = 'added' | 'modified' | 'deleted';

export interface MemoryCommit {
  hash: string;
  subject: string;
}

export type DiffLineKind = 'context' | 'added' | 'removed';

export interface DiffLine {
  kind: DiffLineKind;
  old?: number;
  new?: number;
  text: string;
}

export interface DiffHunk {
  header?: string;
  lines: DiffLine[];
}

export interface DiffFile {
  path: string;
  binary: boolean;
  truncated?: boolean;
  hunks: DiffHunk[];
}

export interface MemoryDiff {
  truncated: boolean;
  files: DiffFile[];
}

const COMMIT_HASH = /^[0-9a-f]{7,40}$/i;

/** Commit rows from a turn body, or the `git-<hash>` id when the body has none. */
export function commitsForMemory(id: string, title: string, commits: string | null): MemoryCommit[] {
  const parsed = commitLines(commits);
  if (parsed.length > 0) return parsed;
  const hash = id.startsWith('git-') ? id.slice(4) : '';
  return COMMIT_HASH.test(hash) ? [{ hash, subject: title }] : [];
}

export function commitLines(commits: string | null): MemoryCommit[] {
  if (!commits) return [];
  const out: MemoryCommit[] = [];
  for (const raw of commits.split('\n')) {
    const trimmed = raw.trim().replace(/^- /, '');
    if (!trimmed) continue;
    const space = trimmed.search(/\s/);
    const hash = space < 0 ? trimmed : trimmed.slice(0, space);
    const subject = space < 0 ? '' : trimmed.slice(space + 1).trim();
    if (COMMIT_HASH.test(hash)) out.push({ hash, subject });
  }
  return out;
}

/** CSS class for one diff line. */
export function diffLineClass(kind: DiffLineKind): string {
  return `memory-diff-line memory-diff-line--${kind}`;
}

/** Place the diff popup fully on screen: left of a right-hand blade, shifted up when the row is near the bottom. */
export function placeDiffPopup(
  anchor: { top: number; left: number; bottom: number },
  viewport: { width: number; height: number },
  popupWidth: number,
  popupHeight = 420,
): { top: number; left: number; width: number; maxHeight: number } {
  const margin = 8;
  const topLimit = 48;
  const bottomLimit = viewport.height - 36;
  const width = Math.min(popupWidth, Math.max(160, viewport.width - margin * 2));
  const maxHeight = Math.min(popupHeight, Math.max(120, bottomLimit - topLimit));
  const left = anchor.left > width + 24
    ? Math.max(margin, anchor.left - width - margin)
    : Math.max(margin, Math.min(anchor.left, viewport.width - width - margin));
  let top = anchor.top;
  if (top + maxHeight > bottomLimit) top = bottomLimit - maxHeight;
  if (top < topLimit) top = topLimit;
  return { top, left, width, maxHeight: Math.min(maxHeight, bottomLimit - top) };
}

/** Center a large diff card in the window, clear of the title bar and the status bar. */
export function centerDiffPopup(
  viewport: { width: number; height: number },
  popupWidth: number,
  popupHeight: number,
): { top: number; left: number; width: number; maxHeight: number } {
  const margin = 24;
  const topLimit = 48;
  const bottomLimit = viewport.height - 36;
  const width = Math.min(popupWidth, Math.max(320, viewport.width - margin * 2));
  const maxHeight = Math.min(popupHeight, Math.max(220, bottomLimit - topLimit));
  const left = Math.max(margin, Math.round((viewport.width - width) / 2));
  const top = Math.max(topLimit, Math.min(bottomLimit - maxHeight, Math.round((viewport.height - maxHeight) / 2)));
  return { top, left, width, maxHeight };
}

/** A click on the commit or inside the popup keeps the diff open. A click anywhere else closes it. */
export function shouldDismissDiffPopup(hit: { onCommit: boolean; insidePopup: boolean }): boolean {
  return !hit.onCommit && !hit.insidePopup;
}

/** File rows open the git host in a new tab when a URL is known. */
export function gitFileLinkProps(
  url: string | undefined,
): { href: string; target: '_blank'; rel: 'noopener noreferrer' } | null {
  if (!url) return null;
  return { href: url, target: '_blank', rel: 'noopener noreferrer' };
}

/** CSS class for a file row. Unknown changes stay uncolored. */
export function fileChangeClass(change: FileChange | undefined): string {
  switch (change) {
    case 'added':
      return 'memory-file--added';
    case 'modified':
      return 'memory-file--modified';
    case 'deleted':
      return 'memory-file--deleted';
    default:
      return '';
  }
}

const OUTCOME_MARKER = '\n\nOutcome:';
const REQUEST_LINE = /^Request:\s+(\S+)$/;
const CONVERSATION_LINE = /^Conversation:\s+([0-9a-fA-F]{12})$/;

/** Split a turn-style memory body into the prompt and the labeled sections after it. */
export function splitMemoryBody(body: string): MemoryBodyParts {
  const normalized = body.replace(/\r\n/g, '\n');
  const markerAt = normalized.indexOf(OUTCOME_MARKER);
  const head = markerAt >= 0 ? normalized.slice(0, markerAt) : normalized.startsWith('Outcome:') ? '' : normalized;
  const outcomeRaw = markerAt >= 0
    ? normalized.slice(markerAt + OUTCOME_MARKER.length)
    : normalized.startsWith('Outcome:')
      ? normalized.slice('Outcome:'.length)
      : '';
  const outcome = outcomeRaw.trim() || null;

  let prompt = '';
  let filesNote: string | null = null;
  let commits: string | null = null;
  let changes: string | null = null;
  let request: string | null = null;
  let conversation: string | null = null;
  const extra: string[] = [];
  for (const raw of head.split(/\n\n+/)) {
    const block = raw.trim();
    if (!block) continue;
    if (block.startsWith('Files:')) {
      filesNote = block.slice('Files:'.length).trim();
    } else if (block.startsWith('Commits:')) {
      commits = block.slice('Commits:'.length).trim();
    } else if (block.startsWith('Changes:')) {
      changes = block.slice('Changes:'.length).trim();
    } else {
      const kept = peelTurnIds(block, (id) => { request = id; }, (id) => { conversation = id; });
      if (!kept) continue;
      if (!prompt) prompt = kept;
      else extra.push(kept);
    }
  }
  if (extra.length > 0) {
    prompt = prompt ? `${prompt}\n\n${extra.join('\n\n')}` : extra.join('\n\n');
  }
  return { prompt, filesNote, commits, changes, request, conversation, outcome };
}

function peelTurnIds(
  block: string,
  onRequest: (id: string) => void,
  onConversation: (id: string) => void,
): string {
  const kept: string[] = [];
  for (const line of block.split('\n')) {
    const request = line.trim().match(REQUEST_LINE);
    const conversation = line.trim().match(CONVERSATION_LINE);
    if (request) onRequest(request[1]);
    else if (conversation) onConversation(conversation[1]);
    else kept.push(line);
  }
  return kept.join('\n').trim();
}

/** Command Center URL for a local image written in a memory; `null` leaves the image as written. */
export function memoryImageUrl(memoryId: string, src: string): string | null {
  const path = src.startsWith('file://') ? src.slice('file://'.length) : src;
  const local = (path.startsWith('/') && !path.startsWith('//')) || /^[A-Za-z]:[\\/]/.test(path);
  if (!local) return null;
  return `/api/memory/${encodeURIComponent(memoryId)}/image?src=${encodeURIComponent(src)}`;
}

/** Number of markdown images (`![alt](target)`) in a memory body. */
export function memoryImageCount(body: string): number {
  return body.match(/!\[[^\]]*\]\([^)\s]+\)/g)?.length ?? 0;
}

export interface ZoomView {
  scale: number;
  x: number;
  y: number;
}

/** Zoom `view` by `factor` around the cursor `(px, py)`, keeping the point under it in place. */
export function zoomAt(view: ZoomView, factor: number, px: number, py: number, min = 1, max = 8): ZoomView {
  const scale = Math.min(max, Math.max(min, view.scale * factor));
  if (scale === min) return { scale, x: 0, y: 0 };
  const ratio = scale / view.scale;
  return { scale, x: px - (px - view.x) * ratio, y: py - (py - view.y) * ratio };
}
