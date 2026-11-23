import { useEffect, useRef } from 'react';
import Codicon from '../components/Codicon';
import MarkdownEditor from '../components/MarkdownEditor';
import type { BodyView } from '../lib/policyBodyView';

export type SpecRevision = {
  version: number;
  status: string;
  source: string;
  body: string;
  hash: string;
  basedOn: number | null;
  createdAt: number;
};

export type EditorMode = BodyView | 'diff';

const MODES: { id: EditorMode; label: string; icon: string }[] = [
  { id: 'markdown', label: 'Markdown', icon: 'code' },
  { id: 'wysiwyg', label: 'WYSIWYG', icon: 'edit' },
  { id: 'split', label: 'Split', icon: 'split-horizontal' },
  { id: 'diff', label: 'Diff', icon: 'diff' },
];

function diffClass(line: string): string {
  if (line.startsWith('@@')) return 'specs-diff-line specs-diff-line--hunk';
  if (line.startsWith('+')) return 'specs-diff-line specs-diff-line--add';
  if (line.startsWith('-')) return 'specs-diff-line specs-diff-line--remove';
  return 'specs-diff-line specs-diff-line--context';
}

type Props = {
  title: string;
  revisions: SpecRevision[];
  current: SpecRevision;
  openVersion: number | null;
  older: SpecRevision[];
  base: SpecRevision | null;
  mode: EditorMode;
  draftText: string;
  dirty: boolean;
  diffText: string;
  onSelectVersion: (version: number) => void;
  onBaseChoice: (version: number) => void;
  onMode: (mode: EditorMode) => void;
  onDraft: (value: string) => void;
  onSave: () => void;
  onDiscard: () => void;
  onApprove?: (version: number) => void;
};

export default function SpecsDetail({
  title,
  revisions,
  current,
  openVersion,
  older,
  base,
  mode,
  draftText,
  dirty,
  diffText,
  onSelectVersion,
  onBaseChoice,
  onMode,
  onDraft,
  onSave,
  onDiscard,
  onApprove,
}: Props) {
  const userEdited = useRef(false);
  useEffect(() => {
    userEdited.current = false;
  }, [current.version, mode]);

  return (
    <article className="specs-revision" aria-label={title}>
      <div className="specs-blade-bar">
        <div className="specs-version-badges">
          <span className="page-item-badge">
            <Codicon name="tag" className="badge-icon" />
            v{current.version}
          </span>
          <span className="page-item-badge">
            <Codicon name={current.status === 'approved' ? 'check' : current.status === 'change_requested' ? 'warning' : 'edit'} className="badge-icon" />
            {current.status.replace(/_/g, ' ')}
          </span>
          <span className="page-item-badge">
            <Codicon name="git-commit" className="badge-icon" />
            {current.source}
          </span>
          {current.basedOn != null && (
            <span className="page-item-badge">
              <Codicon name="git-branch" className="badge-icon" />
              v{current.basedOn}
            </span>
          )}
        </div>
        <div className="specs-version-switch" role="group" aria-label="Open version">
          {revisions.map((row) => (
            <button
              key={row.version}
              type="button"
              className="specs-version-chip"
              aria-pressed={openVersion === row.version}
              onClick={() => onSelectVersion(row.version)}
            >
              v{row.version}
            </button>
          ))}
        </div>
        {older.length > 0 && (
          <div className="specs-version-switch" role="group" aria-label="Compare with version">
            <span className="specs-version-label">Compare</span>
            {older.map((row) => (
              <button
                key={row.version}
                type="button"
                className="specs-version-chip"
                aria-pressed={base?.version === row.version}
                onClick={() => onBaseChoice(row.version)}
              >
                v{row.version}
              </button>
            ))}
          </div>
        )}
        <div className="policy-body-toggle" role="group" aria-label="Body view">
          {MODES.map((item) => (
            <button
              key={item.id}
              type="button"
              className={`btn policy-body-toggle-btn${mode === item.id ? ' policy-body-toggle-btn--active' : ''}`}
              aria-pressed={mode === item.id}
              onClick={() => onMode(item.id)}
            >
              <Codicon name={item.icon} />
              {item.label}
            </button>
          ))}
        </div>
        <div className="specs-actions">
          <button type="button" className="btn btn-compact" disabled={!dirty} onClick={onSave}>
            Save
          </button>
          {dirty && (
            <button type="button" className="btn btn-compact" onClick={onDiscard}>
              Discard
            </button>
          )}
          {onApprove && current.status === 'draft' && !dirty && (
            <button type="button" className="btn btn-compact" onClick={() => onApprove(current.version)}>
              Approve v{current.version}
            </button>
          )}
        </div>
      </div>
      {mode === 'diff' ? (
        !base ? (
          <p role="status">No previous version.</p>
        ) : diffText.length === 0 ? (
          <p role="status">No changes from v{base.version}.</p>
        ) : (
          <pre className="specs-diff" aria-label="Changes from the selected version">
            {diffText.split('\n').map((line, index) => (
              <div key={`${index}-${line}`} className={diffClass(line)}>{line || ' '}</div>
            ))}
          </pre>
        )
      ) : (
        <div
          onKeyDown={() => { userEdited.current = true; }}
          onMouseDown={() => { userEdited.current = true; }}
        >
          <MarkdownEditor
            value={draftText}
            onChange={(value) => {
              if (mode === 'wysiwyg' && !userEdited.current) return;
              onDraft(value);
            }}
            fill
            view={mode}
          />
        </div>
      )}
    </article>
  );
}
