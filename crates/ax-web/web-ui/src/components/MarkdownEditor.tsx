import MDEditor from '@uiw/react-md-editor';

interface Props {
  value: string;
  onChange: (value: string) => void;
  /** Fixed pixel height. Ignored when `fill` is true. */
  height?: number;
  /** Stretch to parent height (rule/skill editor panels). */
  fill?: boolean;
}

export default function MarkdownEditor({ value, onChange, height = 520, fill = false }: Props) {
  return (
    <div className={`md-editor-wrap${fill ? ' md-editor-wrap--fill' : ''}`} data-color-mode="dark">
      <MDEditor
        value={value}
        onChange={(v) => onChange(v ?? '')}
        preview="live"
        height={fill ? '100%' : height}
        visibleDragbar={false}
        textareaProps={{
          spellCheck: false,
          placeholder: 'Write rule or skill content in Markdown…',
        }}
      />
    </div>
  );
}
