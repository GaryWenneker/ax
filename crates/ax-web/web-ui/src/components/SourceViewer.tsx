import { useMemo } from 'react';
import { highlightLine } from '../lib/sourceHighlight';

export interface SourceViewerLine {
  no: number;
  text: string;
}

interface Props {
  lines: SourceViewerLine[];
  language: string;
  highlightRange?: { start: number; end: number };
}

export default function SourceViewer({ lines, language, highlightRange }: Props) {
  const highlighted = useMemo(
    () => lines.map((line) => ({
      no: line.no,
      html: highlightLine(line.text, language),
    })),
    [lines, language],
  );

  return (
    <pre className="detail-code detail-code--source">
      {highlighted.map((line) => (
        <div
          key={line.no}
          className={`source-line${
            highlightRange && line.no >= highlightRange.start && line.no <= highlightRange.end
              ? ' source-line--in-node'
              : ''
          }`}
        >
          <span className="source-line-no">{line.no}</span>
          <span
            className="source-line-text"
            dangerouslySetInnerHTML={{ __html: line.html }}
          />
        </div>
      ))}
    </pre>
  );
}
