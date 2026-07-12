import MDEditor from '@uiw/react-md-editor';

interface Props {
  value: string;
  className?: string;
}

export default function MarkdownPreview({ value, className = '' }: Props) {
  return (
    <div className={`md-preview-wrap${className ? ` ${className}` : ''}`} data-color-mode="dark">
      <MDEditor.Markdown source={value || '_No content._'} />
    </div>
  );
}
