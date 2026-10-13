/**
 * Content-aware column fitting for the shared list columns.
 *
 * The pure functions (`fitWidths`, `titleTrack`, `combineDemands`) decide widths from measured content;
 * `measureList` reads the content demand of a rendered list from the DOM. The result fills the list
 * container exactly: rigid columns (checkboxes, badges, action buttons — every row asks for the same
 * width) keep their natural width, and the spare or missing space is shared by the flexible text
 * columns in proportion to how much content they hold.
 */
export interface ColumnDemand { natural: number; rigid: boolean }

export interface ListMeasurement {
  demands: ColumnDemand[];
  /** Width each heading needs; a column never shrinks below its own heading. */
  headings: number[];
  /** Inner width available for column tracks (container minus padding and gaps). */
  available: number;
  /** Index of the column whose cells split into a title and a description, if any. */
  nameColumn: number;
  title: number;
  description: number;
  gap: number;
}

export interface ColumnFit { widths: number[]; titleTrack: number | null }

const RIGID_TOLERANCE = 2;
const MIN_TITLE = 96;
const TITLE_SHARE_CAP = 0.4;
/** A title column is a column: it keeps at least this share even when the description is long. */
const TITLE_SHARE_MIN = 0.22;
const TITLE_SLACK = 4;

const sum = (values: readonly number[]) => values.reduce((total, value) => total + value, 0);

/** Round to whole pixels and hand the rounding remainder to the given columns so the total is exact. */
function roundTo(widths: number[], total: number, candidates: number[]): number[] {
  const rounded = widths.map(width => Math.floor(width));
  let remainder = total - sum(rounded);
  const order = [...candidates].sort((a, b) => (widths[b] - rounded[b]) - (widths[a] - rounded[a]));
  for (let i = 0; remainder > 0 && order.length > 0; i++, remainder--) rounded[order[i % order.length]] += 1;
  return rounded;
}

export function fitWidths(demands: readonly ColumnDemand[], minimums: readonly number[], available: number): number[] {
  const count = demands.length;
  if (count === 0) return [];
  const minimum = (index: number) => minimums[index] ?? 64;
  const base = demands.map((demand, index) => Math.max(minimum(index), Math.ceil(Number.isFinite(demand.natural) ? demand.natural : 0)));
  const flexible = demands.map(demand => !demand.rigid);
  if (!flexible.some(Boolean)) flexible[base.indexOf(Math.max(...base))] = true;
  const flexibleIndexes = flexible.flatMap((flex, index) => flex ? [index] : []);
  const total = sum(base);
  if (total <= available) {
    // Spare space follows content: a column that already shows everything it has gains little or nothing.
    const spare = available - total;
    const beyondMinimum = flexibleIndexes.some(index => base[index] > minimum(index));
    const share = (index: number) => beyondMinimum ? Math.max(0, base[index] - minimum(index)) : base[index];
    const weight = sum(flexibleIndexes.map(share));
    const widths = base.map((width, index) => flexible[index] ? width + spare * share(index) / weight : width);
    return roundTo(widths, available, flexibleIndexes);
  }
  // Overflow: flexible columns give up the same share of what they asked for, never below their minimum.
  const widths = [...base];
  let shrinking = [...flexibleIndexes];
  for (;;) {
    const deficit = sum(widths) - available;
    if (deficit <= 0 || shrinking.length === 0) break;
    const pool = sum(shrinking.map(index => widths[index]));
    const scale = Math.max(0, 1 - deficit / pool);
    const clamped = shrinking.filter(index => widths[index] * scale < minimum(index));
    if (clamped.length === 0) {
      for (const index of shrinking) widths[index] *= scale;
      break;
    }
    for (const index of clamped) widths[index] = minimum(index);
    shrinking = shrinking.filter(index => !clamped.includes(index));
  }
  const fitted = sum(widths) <= available + 1e-6;
  return fitted ? roundTo(widths, available, shrinking.length ? shrinking : flexibleIndexes) : widths.map(width => Math.ceil(width));
}

/**
 * Width of the title track inside a cell that shows `title · description` on one line.
 * The description stays readable when both cannot fit; the title never collapses below a usable width.
 */
export function titleTrack({title, description, width, gap}: {title: number; description: number; width: number; gap: number}): number {
  const share = Math.round(width * TITLE_SHARE_MIN);
  if (title + description + gap <= width) return Math.max(Math.round(title + TITLE_SLACK), Math.min(share, width - description - gap));
  const floor = Math.min(title + TITLE_SLACK, Math.round(width * TITLE_SHARE_CAP));
  const leavingDescription = Math.min(Math.round(width * title / (title + description || 1)), width - description - gap);
  return Math.max(MIN_TITLE, Math.round(Math.max(floor, leavingDescription, Math.min(share, width * TITLE_SHARE_CAP))));
}

/** Several lists on one page (grouped rules, skills) share one set of widths: the widest content wins. */
export function combineDemands(lists: readonly (readonly ColumnDemand[])[]): ColumnDemand[] {
  const count = Math.max(0, ...lists.map(list => list.length));
  return Array.from({length: count}, (_, index) => {
    const cells = lists.map(list => list[index]).filter((cell): cell is ColumnDemand => Boolean(cell));
    const naturals = cells.map(cell => cell.natural);
    const natural = Math.max(0, ...naturals);
    const rigid = cells.length > 0 && cells.every(cell => cell.rigid) && natural - Math.min(...naturals) <= RIGID_TOLERANCE;
    return {natural, rigid};
  });
}

export function computeFit(measurements: readonly ListMeasurement[], minimums: readonly number[], available: number): ColumnFit | null {
  if (measurements.length === 0) return null;
  const demands = combineDemands(measurements.map(m => m.demands));
  const limits = demands.map((_, index) => Math.max(minimums[index] ?? 64, ...measurements.map(m => m.headings[index] ?? 0)));
  const widths = fitWidths(demands, limits, available);
  const named = measurements.filter(m => m.nameColumn >= 0 && m.title > 0 && m.description > 0);
  if (named.length === 0) return {widths, titleTrack: null};
  const nameColumn = named[0].nameColumn;
  const track = titleTrack({
    title: Math.max(...named.map(m => m.title)),
    description: Math.max(...named.map(m => m.description)),
    width: widths[nameColumn] ?? 0,
    gap: named[0].gap,
  });
  return {widths, titleTrack: track};
}

// ---------------------------------------------------------------------------------------------
// DOM measurement

const px = (value: string) => { const n = parseFloat(value); return Number.isFinite(n) ? n : 0; };
const INTRINSIC = new Set(['INPUT', 'SELECT', 'TEXTAREA', 'IMG', 'svg', 'CANVAS', 'VIDEO']);
const SAMPLE_ROWS = 80;
const MAX_DEPTH = 5;

function textWidth(element: HTMLElement): number {
  const range = element.ownerDocument.createRange();
  range.selectNodeContents(element);
  const rects = Array.from(range.getClientRects());
  if (rects.length === 0) return 0;
  const left = Math.min(...rects.map(rect => rect.left));
  const right = Math.max(...rects.map(rect => rect.right));
  return right - left;
}

/** Fractional border-box width; offsetWidth rounds, and rounded badges summed together wrap one pixel early. */
const boxWidth = (element: HTMLElement) => element.getBoundingClientRect().width;

function horizontalChrome(style: CSSStyleDeclaration): number {
  return px(style.paddingLeft) + px(style.paddingRight) + px(style.borderLeftWidth) + px(style.borderRightWidth);
}

function rowSums(children: HTMLElement[], gap: number, depth: number): number {
  const rows = new Map<number, number[]>();
  for (const child of children) {
    const top = Math.round(child.getBoundingClientRect().top / 4);
    const list = rows.get(top) ?? [];
    list.push(naturalWidth(child, depth + 1));
    rows.set(top, list);
  }
  return Math.max(0, ...Array.from(rows.values(), widths => sum(widths) + gap * Math.max(0, widths.length - 1)));
}

/** The width an element needs to show its content on one line, independent of the width it was given. */
export function naturalWidth(element: HTMLElement, depth = 0): number {
  const style = getComputedStyle(element);
  if (style.display === 'none' || style.position === 'absolute' || style.visibility === 'hidden') return 0;
  const chrome = horizontalChrome(style);
  const inline = style.display.startsWith('inline');
  if (INTRINSIC.has(element.tagName) || (element.tagName === 'BUTTON' && px(style.flexGrow) === 0 && !inline)) return boxWidth(element);
  const children = Array.from(element.children).filter((child): child is HTMLElement => child instanceof HTMLElement);
  const blockChildren = children.filter(child => !getComputedStyle(child).display.startsWith('inline'));
  let content: number;
  if (children.length === 0 || blockChildren.length === 0 || depth >= MAX_DEPTH) {
    content = textWidth(element);
  } else if (style.display.includes('flex') && !style.flexDirection.startsWith('column')) {
    content = sum(children.map(child => naturalWidth(child, depth + 1))) + px(style.columnGap || style.gap) * Math.max(0, children.length - 1);
  } else if (style.display.includes('grid')) {
    content = rowSums(children, px(style.columnGap || style.gap), depth);
  } else {
    content = Math.max(0, ...children.map(child => naturalWidth(child, depth + 1)));
  }
  const natural = Math.ceil(content + chrome);
  // Inline-level boxes (badges, chips, toggles) never stretch, so their rendered width is a true demand.
  return inline || element.tagName === 'BUTTON' ? Math.max(natural, boxWidth(element)) : natural;
}

function headingWidth(heading: HTMLElement): number {
  const style = getComputedStyle(heading);
  const parts = Array.from(heading.children).filter((child): child is HTMLElement => child instanceof HTMLElement && getComputedStyle(child).position !== 'absolute');
  const content = sum(parts.map(part => part.classList.contains('column-sort') ? Math.ceil(textWidth(part) + horizontalChrome(getComputedStyle(part))) : boxWidth(part)));
  return Math.ceil(content + horizontalChrome(style) + px(style.columnGap || style.gap) * Math.max(0, parts.length - 1));
}

/** A cell made only of controls (checkboxes, toggles, action buttons) has a fixed width whatever the row says. */
function controlOnly(cell: HTMLElement): boolean {
  const walker = cell.ownerDocument.createTreeWalker(cell, NodeFilter.SHOW_TEXT);
  for (let node = walker.nextNode(); node; node = walker.nextNode()) {
    if (node.textContent?.trim() && !node.parentElement?.closest('button,input,select,summary,[role="switch"]')) return false;
  }
  return true;
}

function nameParts(cell: HTMLElement): {title: HTMLElement; description: HTMLElement; gap: number} | null {
  const title = cell.querySelector<HTMLElement>(':scope > .page-item-title, :scope > .page-item-title-line');
  const description = cell.querySelector<HTMLElement>(':scope > .page-item-sub');
  if (!title || !description) return null;
  const style = getComputedStyle(cell);
  return {title, description, gap: px(style.columnGap || style.gap) || 12};
}

/** Measure a rendered list: the header row plus up to `SAMPLE_ROWS` rows whose cell count matches the columns. */
export function measureList(list: HTMLElement, columnCount: number): ListMeasurement | null {
  const header = list.querySelector<HTMLElement>(':scope > .column-header');
  if (!header || header.children.length !== columnCount) return null;
  const headerStyle = getComputedStyle(header);
  const gap = px(headerStyle.columnGap || headerStyle.gap);
  const available = header.clientWidth - px(headerStyle.paddingLeft) - px(headerStyle.paddingRight) - gap * Math.max(0, columnCount - 1);
  if (available <= 0) return null;
  // Grid cells only: absolutely positioned decorations (the logging node dot) are not columns.
  const rows = Array.from(list.children)
    .filter((child): child is HTMLElement => child instanceof HTMLElement && child !== header && !child.classList.contains('column-empty'))
    .map(row => Array.from(row.children).filter((cell): cell is HTMLElement => cell instanceof HTMLElement && getComputedStyle(cell).position !== 'absolute'))
    .filter(cells => cells.length === columnCount)
    .slice(0, SAMPLE_ROWS);
  const demands: ColumnDemand[] = [];
  const headings: number[] = [];
  let nameColumn = -1, title = 0, description = 0, nameGap = 12;
  for (let index = 0; index < columnCount; index++) {
    const heading = headingWidth(header.children[index] as HTMLElement);
    headings.push(heading);
    const cells = rows.map(row => row[index]);
    const widths = cells.map(cell => naturalWidth(cell));
    const natural = Math.max(heading, ...widths);
    const rigid = widths.length > 0 && (Math.max(...widths) - Math.min(...widths) <= RIGID_TOLERANCE || cells.every(controlOnly));
    demands.push({natural, rigid});
    for (const cell of cells) {
      const parts = nameParts(cell);
      if (!parts) continue;
      nameColumn = index;
      nameGap = parts.gap;
      title = Math.max(title, naturalWidth(parts.title, 1));
      description = Math.max(description, naturalWidth(parts.description, 1));
    }
  }
  return {demands, headings, available, nameColumn, title, description, gap: nameGap};
}
