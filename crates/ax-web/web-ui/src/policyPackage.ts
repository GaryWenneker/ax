export function allIdsSelected(ids: string[], selected: Set<string>): boolean {
  return ids.length > 0 && ids.every((id) => selected.has(id));
}

/** Select all when any are missing; otherwise clear (Select none). */
export function toggleSelectAll(ids: string[], selected: Set<string>): Set<string> {
  if (allIdsSelected(ids, selected)) return new Set();
  return new Set(ids);
}

export function compareLabel(compare: string): string {
  switch (compare) {
    case 'new':
      return 'New';
    case 'identical':
      return 'Identical';
    case 'changed':
      return 'Different';
    case 'invalid':
      return 'Invalid';
    default:
      return compare;
  }
}

export function policyItemDescription(input: {
  id: string;
  description?: string | null;
  body?: string | null;
}): string {
  const explicit = (input.description ?? '').trim();
  if (explicit) return clipDescription(explicit);
  const fromBody = firstProse(input.body ?? '');
  if (fromBody) return fromBody;
  return humanizePolicyId(input.id);
}

function clipDescription(text: string): string {
  const t = text.replace(/\s+/g, ' ').trim();
  if (t.length <= 220) return t;
  return `${t.slice(0, 217).trimEnd()}…`;
}

function firstProse(body: string): string | null {
  let para = '';
  for (const line of body.split('\n')) {
    let t = line.trim();
    if (!t) {
      if (para) break;
      continue;
    }
    if (t.startsWith('```')) continue;
    t = t.replace(/^#+/, '').replace(/^>/, '').trim();
    if (!t) continue;
    para = para ? `${para} ${t}` : t;
    if (para.length > 220) break;
  }
  return para ? clipDescription(para) : null;
}

function humanizePolicyId(id: string): string {
  return id.replace(/[-_]+/g, ' ').trim() || id;
}

export function compareBadgeClass(compare: string): string {
  const kind = ['new', 'identical', 'changed', 'invalid'].includes(compare) ? compare : 'invalid';
  return `policy-pack-badge policy-pack-badge--${kind}`;
}

export function newerLabel(newer: string): string | null {
  switch (newer) {
    case 'local':
      return 'Local newer';
    case 'package':
      return 'Package newer';
    case 'equal':
      return 'Same age';
    case 'unknown':
      return 'Age unknown';
    default:
      return null;
  }
}

export function newerBadgeClass(newer: string): string {
  const kind = ['local', 'package', 'equal', 'unknown'].includes(newer) ? newer : 'unknown';
  return `policy-pack-badge policy-pack-badge--newer-${kind}`;
}

/** One-line Compare cell: status, then age when it is meaningful. */
export function compareSummary(compare: string, newer?: string | null): string {
  const status = compareLabel(compare);
  const age = newer ? newerLabel(newer) : null;
  return age ? `${status} · ${age}` : status;
}

export function compareStatusClass(compare: string, newer?: string | null): string {
  if (newer === 'local') {
    return 'policy-pack-compare-status policy-pack-compare-status--local';
  }
  const kind = ['new', 'identical', 'changed', 'invalid'].includes(compare) ? compare : 'invalid';
  return `policy-pack-compare-status policy-pack-compare-status--${kind}`;
}

export function emptyDiffCopy(compare: string): string {
  if (compare === 'changed') {
    return 'No line-level diff. The files still differ (line endings or encoding).';
  }
  return 'No differences — local matches the package.';
}

export function restoreDecisionLabels(): { reject: string; accept: string } {
  return { reject: 'Reject', accept: 'Accept' };
}

export type HunkTake = 'local' | 'package' | 'both' | 'none';
export type HunkPick = { index: number; take: HunkTake };
export type RestoreMergeDecision = {
  action: 'merge';
  acceptHunks?: number[];
  hunks?: HunkPick[];
};
export type RestoreDecisionValue = 'overwrite' | 'skip' | RestoreMergeDecision;

export function hunkTakeFromChecks(localOn: boolean, packageOn: boolean): HunkTake {
  if (localOn && packageOn) return 'both';
  if (packageOn) return 'package';
  if (localOn) return 'local';
  return 'none';
}

export function checksFromHunkTake(take: HunkTake): { local: boolean; package: boolean } {
  return {
    local: take === 'local' || take === 'both',
    package: take === 'package' || take === 'both',
  };
}

export function hunkTakesForDecision(decision: RestoreDecisionValue | undefined, hunkCount: number): HunkTake[] {
  const takes: HunkTake[] = Array.from({ length: hunkCount }, () => 'local');
  if (!decision || decision === 'skip') return takes;
  if (decision === 'overwrite') return takes.map(() => 'package');
  for (const i of decision.acceptHunks ?? []) {
    if (i >= 0 && i < hunkCount) takes[i] = 'package';
  }
  for (const pick of decision.hunks ?? []) {
    if (pick.index >= 0 && pick.index < hunkCount) takes[pick.index] = pick.take;
  }
  return takes;
}

export function restoreFileActionLabel(
  decision: RestoreDecisionValue | undefined,
  hunkCount: number,
): 'accept' | 'reject' | 'partial' {
  if (!decision || decision === 'skip') return 'reject';
  if (decision === 'overwrite') return 'accept';
  const takes = hunkTakesForDecision(decision, hunkCount);
  if (takes.length === 0) return 'reject';
  if (takes.every((t) => t === 'local')) return 'reject';
  if (takes.every((t) => t === 'package')) return 'accept';
  return 'partial';
}

export function toRestoreApiDecision(
  decision: RestoreDecisionValue,
  hunkCount: number,
): RestoreDecisionValue {
  const takes = hunkTakesForDecision(decision, hunkCount);
  if (takes.every((t) => t === 'local')) return 'skip';
  if (takes.length > 0 && takes.every((t) => t === 'package')) return 'overwrite';
  const acceptHunks = takes.map((t, i) => (t === 'package' ? i : -1)).filter((i) => i >= 0);
  const hunks = takes.map((take, index) => ({ index, take }));
  return { action: 'merge', acceptHunks, hunks };
}

export function changeNavLabel(index: number, total: number): string {
  if (total <= 0) return 'Change 0 of 0';
  return `Change ${index + 1} of ${total}`;
}

export function setHunkTake(
  decision: RestoreDecisionValue | undefined,
  hunkIndex: number,
  hunkCount: number,
  take: HunkTake,
): RestoreDecisionValue {
  const takes = hunkTakesForDecision(decision, hunkCount);
  if (hunkIndex >= 0 && hunkIndex < hunkCount) takes[hunkIndex] = take;
  return toRestoreApiDecision({ action: 'merge', hunks: takes.map((t, index) => ({ index, take: t })) }, hunkCount);
}

export function numberedHunkLines(
  lines: string[],
  start: number,
): Array<{ n: number | null; text: string }> {
  const rows = lines.length ? lines : [''];
  return rows.map((text, i) => ({
    n: start > 0 ? start + i : null,
    text,
  }));
}

/** First zip in a drag-and-drop FileList, or null if the drop is not a zip. */
export function pickDroppedPolicyZipFile(files: ArrayLike<File> | null | undefined): File | null {
  if (!files || files.length === 0) return null;
  for (let i = 0; i < files.length; i++) {
    const file = files[i];
    const name = file.name.toLowerCase();
    if (
      name.endsWith('.zip') ||
      file.type === 'application/zip' ||
      file.type === 'application/x-zip-compressed'
    ) {
      return file;
    }
  }
  return null;
}

export type UnifiedDiffLineKind = 'meta' | 'add' | 'del' | 'ctx';

export function unifiedDiffLines(unified: string): Array<{ kind: UnifiedDiffLineKind; text: string }> {
  if (!unified) return [];
  const raw = unified.replace(/\n$/, '').split('\n');
  return raw.map((line) => {
    if (line.startsWith('+++') || line.startsWith('---') || line.startsWith('@@')) {
      return { kind: 'meta' as const, text: line };
    }
    if (line.startsWith('+')) return { kind: 'add' as const, text: line };
    if (line.startsWith('-')) return { kind: 'del' as const, text: line };
    return { kind: 'ctx' as const, text: line };
  });
}
