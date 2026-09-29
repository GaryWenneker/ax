import { useEffect, useRef, useState, type PointerEvent } from 'react';
import { createPortal } from 'react-dom';
import Codicon from './Codicon';
import { zoomAt, type ZoomView } from '../memoryDetail';

interface Props {
  src: string;
  alt: string;
  onClose: () => void;
}

const FIT: ZoomView = { scale: 1, x: 0, y: 0 };

export default function ImageLightbox({ src, alt, onClose }: Props) {
  const [view, setView] = useState<ZoomView>(FIT);
  const frameRef = useRef<HTMLDivElement>(null);
  const drag = useRef<{ x: number; y: number } | null>(null);

  useEffect(() => {
    function onKey(e: KeyboardEvent) {
      if (e.key !== 'Escape') return;
      e.stopPropagation();
      onClose();
    }
    window.addEventListener('keydown', onKey, true);
    return () => window.removeEventListener('keydown', onKey, true);
  }, [onClose]);

  useEffect(() => {
    const frame = frameRef.current;
    if (!frame) return;
    function onWheel(e: WheelEvent) {
      e.preventDefault();
      const box = frame!.getBoundingClientRect();
      const factor = Math.exp(-e.deltaY * 0.0015);
      setView((v) => zoomAt(v, factor, e.clientX - box.left, e.clientY - box.top));
    }
    frame.addEventListener('wheel', onWheel, { passive: false });
    return () => frame.removeEventListener('wheel', onWheel);
  }, []);

  function onPointerDown(e: PointerEvent<HTMLDivElement>) {
    if (view.scale === 1) return;
    e.currentTarget.setPointerCapture(e.pointerId);
    drag.current = { x: e.clientX, y: e.clientY };
  }

  function onPointerMove(e: PointerEvent<HTMLDivElement>) {
    const from = drag.current;
    if (!from) return;
    drag.current = { x: e.clientX, y: e.clientY };
    setView((v) => ({ ...v, x: v.x + e.clientX - from.x, y: v.y + e.clientY - from.y }));
  }

  return createPortal(
    <div
      className="image-lightbox"
      role="dialog"
      aria-modal="true"
      aria-label={alt || 'Image'}
      onClick={(e) => {
        if (e.target === e.currentTarget) onClose();
      }}
      onWheel={(e) => e.stopPropagation()}
    >
      <div className="image-lightbox-bar">
        <span className="image-lightbox-caption">{alt}</span>
        <span className="image-lightbox-zoom" title="Scroll to zoom, drag to pan, double-click to reset">
          {Math.round(view.scale * 100)}%
        </span>
        <button type="button" className="detail-close" onClick={onClose} aria-label="Close image" autoFocus>
          <Codicon name="close" />
        </button>
      </div>
      <div
        ref={frameRef}
        className={`image-lightbox-frame${view.scale > 1 ? ' image-lightbox-frame--zoomed' : ''}`}
        onPointerDown={onPointerDown}
        onPointerMove={onPointerMove}
        onPointerUp={() => (drag.current = null)}
        onPointerCancel={() => (drag.current = null)}
        onDoubleClick={() => setView(FIT)}
      >
        <img
          src={src}
          alt={alt}
          draggable={false}
          style={{ transform: `translate(${view.x}px, ${view.y}px) scale(${view.scale})` }}
        />
      </div>
    </div>,
    document.body,
  );
}
