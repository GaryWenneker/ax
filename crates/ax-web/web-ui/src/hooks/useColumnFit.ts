import {useLayoutEffect, useRef, useState, type CSSProperties, type RefObject} from 'react';
import {computeFit, measureList, type ColumnFit, type ListMeasurement} from '../lib/columnFit';

/** Below this container width the lists switch to stacked cards; fitted tracks would fight that layout. */
const MIN_FIT_WIDTH = 760;
const EVENT = 'ax-list-fit';

/** Lists that share a storage key (the groups of one page) publish their demands here and share one fit. */
const registry = new Map<string, Map<number, ListMeasurement>>();
let nextInstance = 1;

function publish(key: string, instance: number, measurement: ListMeasurement | null) {
  const group = registry.get(key) ?? new Map<number, ListMeasurement>();
  if (measurement) group.set(instance, measurement); else group.delete(instance);
  if (group.size) registry.set(key, group); else registry.delete(key);
  window.dispatchEvent(new CustomEvent(EVENT, {detail: {key}}));
}

function sameFit(a: ColumnFit | null, b: ColumnFit | null): boolean {
  return a === b || (a !== null && b !== null && a.titleTrack === b.titleTrack && a.widths.length === b.widths.length && a.widths.every((width, i) => width === b.widths[i]));
}

/**
 * Content-aware automatic column widths for a rendered list.
 * Active only while the user has no manual widths; re-measures when the container width or the rows change.
 */
export function useColumnFit(list: RefObject<HTMLElement>, key: string, columnCount: number, minimums: readonly number[], manual: boolean, rowsKey: string | number) {
  const [fit, setFit] = useState<ColumnFit | null>(null);
  const instance = useRef(0);
  if (instance.current === 0) instance.current = nextInstance++;
  const minimumsKey = minimums.join(',');
  useLayoutEffect(() => {
    const element = list.current;
    const id = instance.current;
    if (!element || manual) { setFit(null); publish(key, id, null); return; }
    const limits = minimumsKey.split(',').map(Number);
    const measure = () => publish(key, id, element.clientWidth >= MIN_FIT_WIDTH ? measureList(element, columnCount) : null);
    const receive = (event: Event) => {
      if ((event as CustomEvent<{key: string}>).detail?.key !== key) return;
      const group = registry.get(key);
      const own = group?.get(id);
      const next = own && group ? computeFit(Array.from(group.values()), limits, own.available) : null;
      setFit(current => sameFit(current, next) ? current : next);
    };
    window.addEventListener(EVENT, receive);
    measure();
    // Text measured in a fallback font is wrong once the web font arrives; measure again then.
    let cancelled = false;
    document.fonts?.ready.then(() => { if (!cancelled) measure(); });
    document.fonts?.addEventListener('loadingdone', measure);
    // Only a width change re-measures: height changes follow from our own widths and must not loop.
    let observedWidth: number | null = null;
    const observer = new ResizeObserver(entries => {
      const width = Math.round(entries[0]?.contentRect.width ?? element.clientWidth);
      if (observedWidth === null) { observedWidth = width; return; }
      if (width === observedWidth) return;
      observedWidth = width;
      measure();
    });
    observer.observe(element);
    return () => {
      cancelled = true;
      document.fonts?.removeEventListener('loadingdone', measure);
      observer.disconnect();
      window.removeEventListener(EVENT, receive);
      publish(key, id, null);
    };
  }, [list, key, columnCount, minimumsKey, manual, rowsKey]);
  const style = fit ? {
    '--list-columns': fit.widths.map(width => `${width}px`).join(' '),
    ...(fit.titleTrack !== null ? {'--title-track': `${fit.titleTrack}px`} : {}),
  } as CSSProperties : undefined;
  return {fit, style};
}
