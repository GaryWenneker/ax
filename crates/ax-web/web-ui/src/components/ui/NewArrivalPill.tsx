import Codicon from '../Codicon';

/** Pending count after a live batch. History and arrivals while already at the newest row stay at the current count. */
export function nextPending(following: boolean, live: boolean, batch: number, pending: number): number {
  if (!live || following || batch <= 0) return pending;
  return pending + batch;
}

export function arrivalLabel(count: number, noun: string): string {
  const word = count === 1 ? noun : `${noun}s`;
  return `${count} new ${word}`;
}

export function NewArrivalPill({
  count,
  noun,
  onJump,
  onDismiss,
  className = '',
}: {
  count: number;
  noun: string;
  onJump: () => void;
  onDismiss: () => void;
  className?: string;
}) {
  if (count <= 0) return null;
  return (
    <div className={`ax-new-pill ${className}`.trim()} role="status">
      <button type="button" className="ax-new-pill-jump" onClick={onJump}>
        <Codicon name="arrow-up" />
        {arrivalLabel(count, noun)}
      </button>
      <button type="button" className="ax-new-pill-dismiss" aria-label="Dismiss" onClick={onDismiss}>
        <Codicon name="close" />
      </button>
    </div>
  );
}
