/**
 * Drag state for the mouse resize handles: a press records the pointer x and
 * the value to start from, each move applies the distance moved, and the body
 * carries `bodyClass` while the drag runs.
 */
export function createMouseDrag<T>(opts: {
  bodyClass: string;
  classList: { add: (c: string) => void; remove: (c: string) => void };
  begin: () => T;
  move: (start: T, dx: number) => void;
}): { down: (clientX: number) => void; move: (clientX: number) => void; up: () => void; dispose: () => void } {
  let dragging = false;
  let startX = 0;
  let start: T | undefined;
  return {
    down: (clientX) => {
      dragging = true;
      startX = clientX;
      start = opts.begin();
      opts.classList.add(opts.bodyClass);
    },
    move: (clientX) => {
      if (!dragging) return;
      opts.move(start as T, clientX - startX);
    },
    up: () => {
      if (!dragging) return;
      dragging = false;
      opts.classList.remove(opts.bodyClass);
    },
    dispose: () => {
      opts.classList.remove(opts.bodyClass);
    },
  };
}
