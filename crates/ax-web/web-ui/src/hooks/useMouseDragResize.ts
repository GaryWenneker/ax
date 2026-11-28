import { useCallback, useEffect, useRef } from 'react';
import { createMouseDrag } from '../lib/mouseDrag';

/**
 * Shared mouse drag for the resize handles. `begin` reads the value to start
 * from on each press; `move` gets that value and the pointer distance moved.
 * Returns the handle's onMouseDown.
 */
export function useMouseDragResize<T>(
  bodyClass: string,
  begin: () => T,
  move: (start: T, dx: number) => void,
): (e: React.MouseEvent) => void {
  const beginRef = useRef(begin);
  const moveRef = useRef(move);
  beginRef.current = begin;
  moveRef.current = move;
  const dragRef = useRef<ReturnType<typeof createMouseDrag<T>> | null>(null);
  if (!dragRef.current) {
    dragRef.current = createMouseDrag<T>({
      bodyClass,
      classList: document.body.classList,
      begin: () => beginRef.current(),
      move: (start, dx) => moveRef.current(start, dx),
    });
  }

  useEffect(() => {
    const drag = dragRef.current!;
    const onMove = (e: MouseEvent) => drag.move(e.clientX);
    const onUp = () => drag.up();
    window.addEventListener('mousemove', onMove);
    window.addEventListener('mouseup', onUp);
    return () => {
      window.removeEventListener('mousemove', onMove);
      window.removeEventListener('mouseup', onUp);
      drag.dispose();
    };
  }, []);

  return useCallback((e: React.MouseEvent) => {
    e.preventDefault();
    dragRef.current!.down(e.clientX);
  }, []);
}
