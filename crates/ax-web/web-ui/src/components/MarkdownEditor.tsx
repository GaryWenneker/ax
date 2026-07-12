import MDEditor from '@uiw/react-md-editor';

interface Props {
  value: string;
  onChange: (value: string) => void;
  height?: number;
}

export default function MarkdownEditor({ value, onChange, height = 520 }: Props) {
  return (
    <div className="md-editor-wrap" data-color-mode="dark">
      <MDEditor
        value={value}
        onChange={(v) => onChange(v ?? '')}
        preview="live"
        height={height}
        visibleDragbar={false}
        textareaProps={{
          spellCheck: false,
          placeholder: 'Write rule or skill content in Markdown…',
        }}
      />
    </div>
  );
}
