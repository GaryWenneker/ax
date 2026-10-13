import {createContext, isValidElement, useEffect, useLayoutEffect, useRef, useState, type CSSProperties, type ReactNode, type KeyboardEvent} from 'react';
import {clampWidth, orderRows, validWidths, type ColumnSort, type ColumnValue} from '../../lib/columnModel';
import MobileControls from './MobileControls';
import useMobile from '../../hooks/useMobile';

export interface ListColumn { label: string; header?: ReactNode; sortable?: boolean; filterable?: boolean; minWidth?: number }
export const ItemColumnContext = createContext<{trace: boolean; labels: readonly string[]} | null>(null);
export function useListColumns(columns: readonly ListColumn[], id: string) {
  const storageKey = `ax-list-columns:v1:${window.location.pathname}:${id}:${columns.map(c => c.label).join('|')}`;
  const minimumsKey = columns.map(c => c.minWidth ?? 64).join(',');
  const minimums = minimumsKey.split(',').map(Number);
  const [widths, setWidths] = useState<number[] | null>(() => {
    try { return validWidths(JSON.parse(localStorage.getItem(storageKey) ?? 'null'), columns.length, minimums); } catch { return null; }
  });
  const [sort, setSort] = useState<ColumnSort | null>(null);
  const [filters, setFilters] = useState<Record<number, string>>({});
  useEffect(() => {
    const limits = minimumsKey.split(',').map(Number);
    try { setWidths(validWidths(JSON.parse(localStorage.getItem(storageKey) ?? 'null'), columns.length, limits)); } catch { setWidths(null); }
    function receive(event: Event) {
      const data = (event as CustomEvent<{key: string; widths: unknown}>).detail;
      if (data?.key === storageKey) setWidths(validWidths(data.widths, columns.length, limits));
    }
    window.addEventListener('ax-list-widths', receive);
    return () => window.removeEventListener('ax-list-widths', receive);
  }, [storageKey, columns.length, minimumsKey]);
  function resize(next: number[] | null) {
    setWidths(next);
    try { if (next) localStorage.setItem(storageKey, JSON.stringify(next)); else localStorage.removeItem(storageKey); } catch { /* Preferences are optional when storage is unavailable. */ }
    window.dispatchEvent(new CustomEvent('ax-list-widths', {detail: {key: storageKey, widths: next}}));
  }
  function toggleSort(index: number) {
    setSort(current => current?.column !== index ? {column: index, direction: 'asc'} : current.direction === 'asc' ? {column: index, direction: 'desc'} : null);
  }
  const style = widths ? {'--list-columns': widths.map(w => `${w}px`).join(' '), '--column-total': `${widths.reduce((sum, width) => sum + width, 0) + (columns.length - 1) * 8 + 24}px`} as CSSProperties : undefined;
  return {columns, widths, resize, sort, filters, toggleSort, setFilter: (index: number, value: string) => setFilters(current => ({...current, [index]: value})), clear: () => {setSort(null); setFilters({}); resize(null);}, style,
    order: <T extends {values: readonly ColumnValue[]}>(rows: readonly T[]) => orderRows(rows, sort, filters)};
}
export type ListColumnControls = ReturnType<typeof useListColumns>;

function filterKeys(event: KeyboardEvent<HTMLDetailsElement>) {
  // Header controls must not trigger the logging list's global keyboard navigation.
  event.stopPropagation();
  if (event.key === 'Escape') {
    event.preventDefault();
    event.currentTarget.open = false;
    event.currentTarget.querySelector('summary')?.focus();
  }
}

function positionFilter(details: HTMLDetailsElement, panel: HTMLDivElement): void {
  const anchor = details.querySelector('summary')?.getBoundingClientRect();
  if (!anchor) return;
  const bounds = details.closest('.page-item-list,.page-table-wrap,.review-column-list')?.getBoundingClientRect();
  const left = Math.max(8, bounds?.left ?? 8);
  const right = Math.min(window.innerWidth - 8, bounds?.right ?? window.innerWidth - 8);
  const width = Math.min(190, Math.max(100, right - left));
  Object.assign(panel.style, {position:'fixed', margin:'0', right:'auto', bottom:'auto', width:`${width}px`, left:`${Math.max(left, Math.min(anchor.right - width, right - width))}px`});
  panel.style.top = `${Math.max(8, Math.min(anchor.bottom + 4, window.innerHeight - panel.getBoundingClientRect().height - 8))}px`;
}

function ColumnFilter({controls, index}: {controls: ListColumnControls; index: number}) {
  const details = useRef<HTMLDetailsElement>(null);
  const panel = useRef<HTMLDivElement>(null);
  const label = controls.columns[index].label;
  useLayoutEffect(() => {
    // React 18's DOM typings predate popover; configure the native top layer through its DOM API.
    if (panel.current && typeof panel.current.showPopover === 'function') panel.current.popover = 'manual';
  }, []);
  useEffect(() => {
    const reposition = () => {
      if (details.current && panel.current && typeof panel.current.showPopover === 'function' && panel.current.matches(':popover-open')) positionFilter(details.current, panel.current);
    };
    const dismiss = (event: Event) => {
      const widget = details.current, popup = panel.current;
      if (!widget?.open || !(event.target instanceof Node) || widget.contains(event.target)) return;
      widget.open = false;
      if (popup && typeof popup.hidePopover === 'function' && popup.matches(':popover-open')) popup.hidePopover();
    };
    window.addEventListener('resize', reposition);
    window.addEventListener('scroll', reposition, {capture:true, passive:true});
    document.addEventListener('pointerdown', dismiss);
    document.addEventListener('focusin', dismiss);
    return () => {
      window.removeEventListener('resize', reposition);
      window.removeEventListener('scroll', reposition, true);
      document.removeEventListener('pointerdown', dismiss);
      document.removeEventListener('focusin', dismiss);
    };
  }, []);
  return <details ref={details} className="column-filter" onKeyDown={filterKeys} onToggle={event => {
    if (event.target !== event.currentTarget || !panel.current || typeof panel.current.showPopover !== 'function') return;
    if (event.currentTarget.open) {panel.current.showPopover(); positionFilter(event.currentTarget, panel.current);}
    else if (panel.current.matches(':popover-open')) panel.current.hidePopover();
  }}>
    <summary aria-label={`Filter ${label}`} title={`Filter ${label}`}>⌕</summary>
    <div ref={panel} className="column-filter-panel"><label>{label}<input autoComplete="off" aria-label={`Filter ${label}`} placeholder="Contains…" value={controls.filters[index] ?? ''} onChange={e => controls.setFilter(index, e.target.value)} /></label></div>
  </details>;
}

export function ColumnTools({controls}: {controls: ListColumnControls}) {
  const mobile = useMobile();
  const active = controls.sort !== null || Object.values(controls.filters).some(v => v.trim());
  const widths = controls.widths;
  const panel = <div className="column-tools-panel">
    <span className="column-tools-note">Sort and filter loaded rows.</span>
    {controls.columns.map((column, index) => <div className="column-tool" key={column.label}>
      <span>{column.label}</span>
      {column.sortable !== false && <button type="button" className="column-sort" onClick={() => controls.toggleSort(index)} aria-label={`Sort ${column.label}`}>{controls.sort?.column === index ? controls.sort.direction === 'asc' ? 'Ascending ↑' : 'Descending ↓' : 'Original order'}</button>}
      {column.filterable !== false && <input aria-label={`Filter ${column.label}`} placeholder="Contains…" value={controls.filters[index] ?? ''} onChange={e => controls.setFilter(index, e.target.value)} />}
      {widths && <input type="number" min={column.minWidth ?? 64} max="1600" aria-label={`${column.label} column width`} value={widths[index]} onChange={e => { const value = e.target.valueAsNumber; if (Number.isFinite(value)) controls.resize(widths.map((w, i) => i === index ? clampWidth(value, column.minWidth ?? 64) : w)); }} />}
    </div>)}
    <button type="button" className="btn" onClick={controls.clear}>Reset columns</button>
  </div>;
  return mobile ? <MobileControls className="mobile-column-tools" title="Sort and filter" active={active}>{panel}</MobileControls>
    : <details className="column-tools" onKeyDown={filterKeys}><summary>Columns{active ? ' · active' : ''}</summary>{panel}</details>;
}

export function ColumnHeading({controls, index}: {controls: ListColumnControls; index: number}) {
  const cell = useRef<HTMLDivElement>(null);
  const drag = useRef<{x: number; widths: number[]} | null>(null);
  const column = controls.columns[index];
  const [measuredWidth, setMeasuredWidth] = useState(64);
  useEffect(() => {
    const element = cell.current;
    if (!element) return;
    const measure = () => setMeasuredWidth(Math.round((element.closest('th') ?? element).getBoundingClientRect().width));
    measure();
    const observer = new ResizeObserver(measure);
    observer.observe(element);
    return () => observer.disconnect();
  }, []);
  const measured = () => {
    if (controls.widths) return controls.widths;
    const parent = cell.current?.closest('tr') ?? cell.current?.parentElement;
    return parent ? Array.from(parent.children, (child, i) => clampWidth(child.getBoundingClientRect().width, controls.columns[i]?.minWidth ?? 64)) : controls.columns.map(c => c.minWidth ?? 64);
  };
  const resize = (delta: number, original = measured()) => controls.resize(original.map((width, i) => i === index ? clampWidth(width + delta, column.minWidth ?? 64) : width));
  return <div ref={cell} className="column-heading" style={{minWidth:column.minWidth ?? 64}} role="group" aria-label={`${column.label} column`}>
    {column.sortable === false ? <span title={column.label}>{column.header ?? column.label}</span> : <button type="button" className="column-sort" onClick={() => controls.toggleSort(index)} aria-label={`Sort ${column.label}`} aria-pressed={controls.sort?.column === index} title={`${controls.sort?.column === index ? `Sorted ${controls.sort.direction}. ` : ''}Click to sort loaded rows; click again to reverse, then restore original order`}>{column.label}{controls.sort?.column === index ? controls.sort.direction === 'asc' ? ' ↑' : ' ↓' : ''}</button>}
    {column.filterable !== false && <ColumnFilter controls={controls} index={index} />}
    <span className="column-resizer" role="separator" aria-orientation="vertical" aria-label={`Resize ${column.label} column`} aria-valuemin={column.minWidth ?? 64} aria-valuemax={1600} aria-valuenow={controls.widths?.[index] ?? measuredWidth} tabIndex={0} title="Drag or use arrow keys to resize. Double-click or Home restores automatic widths."
      onPointerDown={e => {if(e.button !== 0) return; e.preventDefault(); e.stopPropagation(); e.currentTarget.focus({preventScroll:true}); drag.current = {x: e.clientX, widths: measured()}; e.currentTarget.setPointerCapture(e.pointerId);}}
      onPointerMove={e => {if(drag.current) resize(e.clientX-drag.current.x, drag.current.widths);}}
      onPointerUp={e => {drag.current = null; if(e.currentTarget.hasPointerCapture(e.pointerId)) e.currentTarget.releasePointerCapture(e.pointerId);}}
      onLostPointerCapture={() => {drag.current = null;}}
      onDoubleClick={() => controls.resize(null)}
      onKeyDown={e => {if(e.key === 'ArrowLeft' || e.key === 'ArrowRight') {e.preventDefault(); e.stopPropagation(); resize(e.key === 'ArrowRight' ? 10 : -10);} else if(e.key === 'Home') {e.preventDefault(); e.stopPropagation(); controls.resize(null);}}} />
  </div>;
}

export function ColumnHeader({controls, className = ''}: {controls: ListColumnControls; className?: string}) {
  return <div className={`column-header ${className}`} role="group" aria-label="List columns">{controls.columns.map((column, index) => <ColumnHeading key={column.label} controls={controls} index={index} />)}</div>;
}

/** Extract text from rendered cell children without invoking components or their hooks. */
export function columnText(node: ReactNode): string {
  if (typeof node === 'string' || typeof node === 'number') return String(node);
  if (Array.isArray(node)) return node.map(columnText).join(' ');
  if (isValidElement<{children?: ReactNode; level?: string; scope?: string; items?: string[]; 'data-column-value'?: string | number}>(node)) {
    const props = node.props;
    if (props.items) return props.items.join(' ');
    return String(props['data-column-value'] ?? props.level ?? props.scope ?? columnText(props.children));
  }
  return '';
}
