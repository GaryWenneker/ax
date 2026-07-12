/** Detect status strings that represent an in-progress operation. */
export function isLiveStatus(value: string): boolean {
  return /running|scanning|evaluating|installing|starting|stopping|loading|creating|saving|regenerating|setting up|in progress|preparing|bootstrapping|importing|matching|waiting for/i.test(
    value,
  );
}

export function Spinner({ size = 'sm' }: { size?: 'xs' | 'sm' | 'md' }) {
  return <span className={`ax-spinner ax-spinner--${size}`} role="status" aria-hidden="true" />;
}

/** Inline spinner + label for buttons and compact busy states. */
export function BusyLabel({ label }: { label: string }) {
  return (
    <span className="ax-busy-label">
      <Spinner />
      {label}
    </span>
  );
}
