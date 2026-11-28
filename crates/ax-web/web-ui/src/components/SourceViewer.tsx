import { useEffect, useMemo, useState } from 'react';
import { highlightLine, languageReady, loadLanguage } from '../lib/sourceHighlight';

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
  // Plain text first; the grammar chunk loads in the background and the lines are colored when it arrives.
  const [ready, setReady] = useState(() => languageReady(language));
  useEffect(() => {
    let live = true;
    setReady(languageReady(language));
    void loadLanguage(language).then((ok) => {
      if (live && ok) setReady(true);
    });
    return () => {
      live = false;
    };
  }, [language]);
  const highlighted = useMemo(
    () => lines.map((line) => ({
      no: line.no,
      html: highlightLine(line.text, language),
    })),
    // `ready` re-runs the highlight once the grammar has loaded.
    [lines, language, ready],
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
