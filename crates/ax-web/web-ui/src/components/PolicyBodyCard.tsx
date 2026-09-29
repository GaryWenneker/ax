import { useState } from 'react';
import Codicon from './Codicon';
import MarkdownEditor from './MarkdownEditor';
import { Backlinks } from './LinkedBody';
import type { ItemLinks } from '../wikilinks';
import { loadBodyView, saveBodyView, type BodyView } from '../lib/policyBodyView';

const VIEWS: { id: BodyView; label: string; icon: string }[] = [
  { id: 'markdown', label: 'Markdown', icon: 'code' },
  { id: 'wysiwyg', label: 'WYSIWYG', icon: 'edit' },
  { id: 'preview', label: 'Preview', icon: 'open-preview' },
];

/** Rule or skill body in a blade: Markdown source, rich-text editor, or rendered preview, one at a time. */
export default function PolicyBodyCard({
  title,
  body,
  onChange,
  links,
  selfKey,
}: {
  title: string;
  body: string;
  onChange: (value: string) => void;
  links: ItemLinks | null | undefined;
  selfKey?: string;
}) {
  const [view, setView] = useState<BodyView>(() => loadBodyView());
  const choose = (next: BodyView) => {
    setView(next);
    saveBodyView(next);
  };

  return (
    <section className="settings-card policy-inline-pane policy-inline-pane--editor page-md-panel">
      <div className="settings-card-header policy-body-header">
        <h2>{title}</h2>
        <div className="policy-body-toggle" role="group" aria-label="Body view">
          {VIEWS.map((v) => (
            <button
              key={v.id}
              type="button"
              className={`btn policy-body-toggle-btn${view === v.id ? ' policy-body-toggle-btn--active' : ''}`}
              aria-pressed={view === v.id}
              onClick={() => choose(v.id)}
            >
              <Codicon name={v.icon} />
              {v.label}
            </button>
          ))}
        </div>
      </div>
      <MarkdownEditor value={body} onChange={onChange} fill links={links} view={view} selfKey={selfKey} />
      <Backlinks backlinks={links?.backlinks} />
    </section>
  );
}
