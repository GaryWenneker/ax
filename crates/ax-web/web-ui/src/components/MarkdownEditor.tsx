import { useRef } from 'react';
import MDEditor from '@uiw/react-md-editor';
import MarkdownPreview from './MarkdownPreview';
import { MdPreviewResizeHandle, loadMdEditPct } from './PolicyEditorResize';

interface Props {
  value: string;
  onChange: (value: string) => void;
  /** Fixed pixel height. Ignored when `fill` is true. */
  height?: number;
  /** Stretch to parent height (rule/skill editor panels). */
  fill?: boolean;
}

/**
 * Source + preview side-by-side with a real vertical resize handle.
 * (uiw's visibleDragbar only resizes editor height, and is disabled when height is %.)
 */
export default function MarkdownEditor({ value, onChange, height = 520, fill = false }: Props) {
  const splitRef = useRef<HTMLDivElement>(null);
  const pct = loadMdEditPct();

  return (
    <div
      ref={splitRef}
      className={`md-editor-wrap md-editor-split${fill ? ' md-editor-wrap--fill' : ''}`}
      data-color-mode="dark"
      style={{ ['--md-edit-pct' as string]: `${pct}%`, height: fill ? undefined : height }}
    >
      <div className="md-editor-split-edit">
        <MDEditor
          value={value}
          onChange={(v) => onChange(v ?? '')}
          preview="edit"
          height="100%"
          visibleDragbar={false}
          textareaProps={{
            spellCheck: false,
            placeholder: 'Write rule or skill content in Markdown…',
          }}
        />
      </div>
      <MdPreviewResizeHandle containerRef={splitRef} />
      <div className="md-editor-split-preview">
        <MarkdownPreview value={value} />
      </div>
    </div>
  );
}
