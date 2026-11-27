/** Pending count after a live batch. History and arrivals while already at the newest row stay at the current count. */
export function nextPending(following: boolean, live: boolean, batch: number, pending: number): number {
  if (!live || following || batch <= 0) return pending;
  return pending + batch;
}

export function arrivalLabel(count: number, noun: string): string {
  const word = count === 1 ? noun : `${noun}s`;
  return `${count} new ${word}`;
}
