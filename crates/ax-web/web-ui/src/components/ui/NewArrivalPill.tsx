import Codicon from '../Codicon';

import { arrivalLabel } from './newArrival';

export { arrivalLabel, nextPending } from './newArrival';

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
