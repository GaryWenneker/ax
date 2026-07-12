import { loadNumber, saveNumber } from '../lib/uiStorage';

const PREVIEW_SCALE_KEY = 'preview-scale';
const MIN = 0.75;
const MAX = 1.5;
const STEP = 0.1;
const DEFAULT = 1;

export function loadPreviewScale(): number {
  return loadNumber(PREVIEW_SCALE_KEY, DEFAULT, MIN, MAX);
}

export function adjustPreviewScale(delta: number): number {
  const next = Math.min(MAX, Math.max(MIN, loadPreviewScale() + delta));
  saveNumber(PREVIEW_SCALE_KEY, next);
  return next;
}

export function PreviewZoomToolbar({
  scale,
  onScaleChange,
  label = 'Preview',
}: {
  scale: number;
  onScaleChange: (s: number) => void;
  label?: string;
}) {
  return (
    <div className="preview-zoom-bar">
      <span className="preview-zoom-label">{label}</span>
      <button
        type="button"
        className="font-btn"
        onClick={() => onScaleChange(adjustPreviewScale(-STEP))}
        title="Smaller preview"
        aria-label="Smaller preview"
      >
        <i className="codicon codicon-remove" aria-hidden="true" />
      </button>
      <span className="font-size-lbl">{Math.round(scale * 100)}%</span>
      <button
        type="button"
        className="font-btn"
        onClick={() => onScaleChange(adjustPreviewScale(STEP))}
        title="Larger preview"
        aria-label="Larger preview"
      >
        <i className="codicon codicon-add" aria-hidden="true" />
      </button>
    </div>
  );
}
