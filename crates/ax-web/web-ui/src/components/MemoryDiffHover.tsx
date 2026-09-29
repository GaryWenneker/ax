import { useEffect, useRef, useState, type ReactNode } from 'react';
import { createPortal } from 'react-dom';
import { fetchMemoryDiff } from '../api';
import { centerDiffPopup, shouldDismissDiffPopup, type DiffLineKind, type MemoryDiff } from '../memoryDetail';

const cache = new Map<string, MemoryDiff>();

function cacheKey(id: string, path?: string, commit?: string): string {
  return `${id}\0${commit ?? ''}\0${path ?? ''}`;
}

/**
 * Opens a file or commit diff. With `clickToOpen`, only a click (or Enter/Space) opens it.
 */
export function MemoryDiffHover({
  memoryId,
  path,
  commit,
  heading,
  className,
  clickToOpen = false,
  children,
}: {
  memoryId: string;
  path?: string;
  commit?: string;
  heading: string;
  className?: string;
  /** When set, hover and focus do not open the diff. A click does. */
  clickToOpen?: boolean;
  children: ReactNode;
}) {
  const anchor = useRef<HTMLDivElement>(null);
  const popupRef = useRef<HTMLDivElement>(null);
  const pinned = useRef(false);
  const blockHover = useRef(false);
  const [open, setOpen] = useState(false);
  const [pos, setPos] = useState<{ top: number; left: number; width: number; maxHeight: number } | null>(null);
  const [diff, setDiff] = useState<MemoryDiff | null>(null);
  const [error, setError] = useState(false);
  const showTimer = useRef<number>(0);
  const hideTimer = useRef<number>(0);

  const place = () => {
    setPos(centerDiffPopup({ width: window.innerWidth, height: window.innerHeight }, 980, 760));
    setOpen(true);
  };

  const show = () => {
    if (blockHover.current) return;
    window.clearTimeout(hideTimer.current);
    window.clearTimeout(showTimer.current);
    showTimer.current = window.setTimeout(place, 150);
  };

  const hidePreview = () => {
    if (pinned.current || blockHover.current) return;
    window.clearTimeout(showTimer.current);
    window.clearTimeout(hideTimer.current);
    hideTimer.current = window.setTimeout(() => setOpen(false), 160);
  };

  const leaveAnchor = () => {
    blockHover.current = false;
    hidePreview();
  };

  const pin = () => {
    blockHover.current = false;
    pinned.current = true;
    window.clearTimeout(hideTimer.current);
    window.clearTimeout(showTimer.current);
    place();
  };

  const dismiss = () => {
    blockHover.current = true;
    pinned.current = false;
    window.clearTimeout(showTimer.current);
    window.clearTimeout(hideTimer.current);
    setOpen(false);
  };

  useEffect(() => {
    if (!open) return;
    const key = cacheKey(memoryId, path, commit);
    const cached = cache.get(key);
    if (cached) {
      setDiff(cached);
      setError(false);
      return;
    }
    let cancelled = false;
    setDiff(null);
    setError(false);
    fetchMemoryDiff(memoryId, { path, commit })
      .then((result) => {
        cache.set(key, result);
        if (!cancelled) setDiff(result);
      })
      .catch(() => {
        if (!cancelled) setError(true);
      });
    return () => {
      cancelled = true;
    };
  }, [open, memoryId, path, commit]);

  useEffect(() => {
    if (!open) return;
    const onPointerDown = (event: PointerEvent) => {
      const target = event.target;
      const onCommit = target instanceof Node && !!anchor.current?.contains(target);
      const insidePopup = target instanceof Node && !!popupRef.current?.contains(target);
      if (!shouldDismissDiffPopup({ onCommit, insidePopup })) return;
      pinned.current = false;
      setOpen(false);
    };
    document.addEventListener('pointerdown', onPointerDown, true);
    return () => document.removeEventListener('pointerdown', onPointerDown, true);
  }, [open]);

  useEffect(() => {
    if (!open) return;
    const onKey = (event: KeyboardEvent) => {
      if (event.key !== 'Escape') return;
      event.stopPropagation();
      dismiss();
    };
    window.addEventListener('keydown', onKey, true);
    return () => window.removeEventListener('keydown', onKey, true);
  }, [open]);

  useEffect(() => () => {
    window.clearTimeout(showTimer.current);
    window.clearTimeout(hideTimer.current);
  }, []);

  return (
    <div
      ref={anchor}
      className={className}
      tabIndex={0}
      onMouseEnter={clickToOpen ? undefined : show}
      onMouseLeave={clickToOpen ? undefined : leaveAnchor}
      onFocus={clickToOpen ? undefined : show}
      onClick={pin}
      onKeyDown={(event) => {
        if (event.key === 'Escape') dismiss();
        if (clickToOpen && (event.key === 'Enter' || event.key === ' ')) {
          event.preventDefault();
          pin();
        }
      }}
    >
      {children}
      {open && pos && createPortal(
        <div className="memory-diff-scrim" onMouseDown={(event) => { if (event.target === event.currentTarget) dismiss(); }}>
          <div
            ref={popupRef}
            className="memory-diff-pop"
            role="dialog"
            aria-label={heading}
            style={{ top: pos.top, left: pos.left, width: pos.width, height: pos.maxHeight }}
            onMouseEnter={() => window.clearTimeout(hideTimer.current)}
            onMouseLeave={hidePreview}
          >
            <div className="memory-diff-head">
              <span className="memory-diff-head-text">
                {commit ? <span className="memory-diff-hash">{commit}</span> : null}
                <span>{commit ? heading.slice(commit.length).trim() : heading}</span>
              </span>
              <button
                type="button"
                className="memory-diff-close"
                onMouseDown={(event) => {
                  event.preventDefault();
                  event.stopPropagation();
                  dismiss();
                }}
              >
                Close
              </button>
            </div>
            <DiffBody diff={diff} error={error} />
          </div>
        </div>,
        document.body,
      )}
    </div>
  );
}

function DiffBody({ diff, error }: { diff: MemoryDiff | null; error: boolean }) {
  const [active, setActive] = useState(0);
  const tabsRef = useRef<HTMLDivElement>(null);
  useEffect(() => setActive(0), [diff]);
  useEffect(() => {
    const list = tabsRef.current;
    const selected = list?.querySelector<HTMLElement>('[aria-selected="true"]');
    if (!list || !selected) return;
    const left = selected.offsetLeft;
    const right = left + selected.offsetWidth;
    if (left < list.scrollLeft) list.scrollLeft = left;
    else if (right > list.scrollLeft + list.clientWidth) list.scrollLeft = right - list.clientWidth;
  }, [active, diff]);
  if (error) return <p className="memory-diff-note">Diff unavailable.</p>;
  if (!diff) return <p className="memory-diff-note">Loading diff…</p>;
  if (diff.files.length === 0) return <p className="memory-diff-note">No line changes for this item.</p>;
  const index = Math.min(active, diff.files.length - 1);
  const file = diff.files[index];
  const stats = tally(file);
  return (
    <>
      {diff.files.length > 1 && (
        <div ref={tabsRef} className="memory-diff-tabs" role="tablist" aria-label="Files in this commit">
          {diff.files.map((item, itemIndex) => {
            const count = tally(item);
            return (
              <button
                key={`${item.path}-${itemIndex}`}
                type="button"
                role="tab"
                aria-selected={itemIndex === index}
                className={`memory-diff-tab memory-diff-tab--${tabKind(count)}`}
                title={item.path}
                onClick={() => setActive(itemIndex)}
              >
                <span className="memory-diff-tab-name">{baseName(item.path)}</span>
                <Count added={count.added} removed={count.removed} />
              </button>
            );
          })}
        </div>
      )}
      <div className="memory-diff-path">
        <span className="memory-diff-path-name" title={file.path}>{file.path}</span>
        <Count added={stats.added} removed={stats.removed} />
      </div>
      <div className="memory-diff-scroll">
        <FileDiff file={file} />
      </div>
    </>
  );
}

function FileDiff({ file }: { file: MemoryDiff['files'][number] }) {
  if (file.binary) return <p className="memory-diff-note">Binary file.</p>;
  if (file.hunks.length === 0) return <p className="memory-diff-note">No line changes.</p>;
  return (
    <>
      {file.hunks.map((hunk, index) => (
        <div key={index} className="memory-diff-hunk">
          {hunk.header && <div className="memory-diff-hunk-head">{hunk.header}</div>}
          {hunk.lines.map((line, lineIndex) => (
            <div key={lineIndex} className={`memory-diff-line memory-diff-line--${line.kind}`}>
              <span className="memory-diff-no">{line.old ?? ''}</span>
              <span className="memory-diff-no">{line.new ?? ''}</span>
              <span className="memory-diff-sign" aria-hidden="true">{sign(line.kind)}</span>
              <span className="memory-diff-text">{line.text}</span>
            </div>
          ))}
        </div>
      ))}
      {file.truncated && (
        <p className="memory-diff-note">This file&apos;s diff is longer than 4000 lines. The rest is not shown.</p>
      )}
    </>
  );
}

function Count({ added, removed }: { added: number; removed: number }) {
  return (
    <span className="memory-diff-stat">
      <span className="memory-diff-add">+{added}</span>
      <span className="memory-diff-del">−{removed}</span>
    </span>
  );
}

function tally(file: MemoryDiff['files'][number]): { added: number; removed: number } {
  let added = 0;
  let removed = 0;
  for (const hunk of file.hunks) {
    for (const line of hunk.lines) {
      if (line.kind === 'added') added += 1;
      if (line.kind === 'removed') removed += 1;
    }
  }
  return { added, removed };
}

function tabKind(count: { added: number; removed: number }): 'added' | 'removed' | 'modified' {
  if (count.added > 0 && count.removed === 0) return 'added';
  if (count.removed > 0 && count.added === 0) return 'removed';
  return 'modified';
}

function baseName(path: string): string {
  const slash = path.lastIndexOf('/');
  return slash < 0 ? path : path.slice(slash + 1);
}

function sign(kind: DiffLineKind): string {
  if (kind === 'added') return '+';
  if (kind === 'removed') return '−';
  return ' ';
}
