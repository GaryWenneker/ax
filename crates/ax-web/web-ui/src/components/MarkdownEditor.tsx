import { useId, useRef, useState } from 'react';
import MDEditor from '@uiw/react-md-editor';
import MarkdownPreview from './MarkdownPreview';
import { MdPreviewResizeHandle, loadMdEditPct } from './PolicyEditorResize';
import { interceptLinkClick } from './LinkedBody';
import LinkPickerList, { optionId, stepIndex, useLinkSearch, type PickerRow } from './LinkPicker';
import WysiwygEditor from './WysiwygEditor';
import { wikiLinksToMarkdown, type ItemLinks } from '../wikilinks';
import { linkText, openLinkQuery } from '../lib/linkPicker';
import type { BodyView } from '../lib/policyBodyView';

interface Props {
  value: string;
  onChange: (value: string) => void;
  /** Fixed pixel height. Ignored when `fill` is true. */
  height?: number;
  /** Stretch to parent height (rule/skill editor panels). */
  fill?: boolean;
  /** Makes `[[links]]` in the preview clickable. */
  links?: ItemLinks | null;
  /** Show only the source, the rich-text editor, or the preview. Omitted: source and preview side by side. */
  view?: BodyView;
  /** `kind:origin:id` of the item being edited; left out of the `[[` picker. */
  selfKey?: string;
}

/**
 * Source + preview side-by-side with a real vertical resize handle.
 * (uiw's visibleDragbar only resizes editor height, and is disabled when height is %.)
 */
export default function MarkdownEditor({ value, onChange, height = 520, fill = false, links, view, selfKey }: Props) {
  const splitRef = useRef<HTMLDivElement>(null);
  const pct = loadMdEditPct();
  const preview = links ? wikiLinksToMarkdown(value, links.outgoing) : value;
  const showEdit = view === undefined || view === 'markdown';
  const showPreview = view === undefined || view === 'preview';

  return (
    <div
      ref={splitRef}
      className={`md-editor-wrap md-editor-split${view ? ' md-editor-split--single' : ''}${fill ? ' md-editor-wrap--fill' : ''}`}
      data-color-mode="dark"
      style={{ ['--md-edit-pct' as string]: `${pct}%`, height: fill ? undefined : height }}
    >
      {showEdit && <SourceEditor value={value} onChange={onChange} selfKey={selfKey} />}
      {view === 'wysiwyg' && (
        <div className="md-editor-split-edit">
          <WysiwygEditor value={value} onChange={onChange} selfKey={selfKey} />
        </div>
      )}
      {!view && <MdPreviewResizeHandle containerRef={splitRef} />}
      {showPreview && (
        <div className="md-editor-split-preview">
          <MarkdownPreview value={preview} onClick={links ? interceptLinkClick : undefined} />
        </div>
      )}
    </div>
  );
}

function SourceEditor({ value, onChange, selfKey }: { value: string; onChange: (v: string) => void; selfKey?: string }) {
  const listId = useId();
  const [open, setOpen] = useState<{ start: number; query: string; caret: number } | null>(null);
  const [index, setIndex] = useState(0);
  const search = useLinkSearch(open !== null, open?.query ?? '', selfKey);
  const items = search.rows;
  const textRef = useRef<HTMLTextAreaElement | null>(null);

  const track = (el: HTMLTextAreaElement) => {
    textRef.current = el;
    const q = openLinkQuery(el.value, el.selectionStart);
    if (q && q.query !== open?.query) setIndex(0);
    setOpen(q ? { ...q, caret: el.selectionStart } : null);
  };

  const insert = (row: PickerRow) => {
    if (!open || row.type !== 'target') return;
    const text = linkText(row.item);
    const after = value.slice(open.caret).startsWith(']]') ? open.caret + 2 : open.caret;
    onChange(value.slice(0, open.start) + text + value.slice(after));
    setOpen(null);
    const caret = open.start + text.length;
    requestAnimationFrame(() => {
      const el = textRef.current;
      if (!el) return;
      el.focus();
      el.setSelectionRange(caret, caret);
    });
  };

  const onKeyDown = (e: React.KeyboardEvent<HTMLTextAreaElement>) => {
    if (!open) return;
    if (e.key === 'Escape') {
      e.preventDefault();
      e.stopPropagation();
      setOpen(null);
      return;
    }
    const next = stepIndex(e.key, index, items.length);
    if (next !== null) {
      e.preventDefault();
      setIndex(next);
      return;
    }
    if ((e.key === 'Enter' || e.key === 'Tab') && items[index]) {
      e.preventDefault();
      insert(items[index]);
    }
  };

  return (
    <div className="md-editor-split-edit md-editor-source">
      <MDEditor
        value={value}
        onChange={(v) => onChange(v ?? '')}
        preview="edit"
        height="100%"
        visibleDragbar={false}
        textareaProps={{
          spellCheck: false,
          placeholder: 'Write rule or skill content in Markdown…',
          'aria-autocomplete': 'list',
          'aria-expanded': open !== null,
          'aria-controls': open ? listId : undefined,
          'aria-activedescendant': open && items[index] ? optionId(listId, index) : undefined,
          onKeyDown,
          onKeyUp: (e) => {
            if (e.key === 'Escape') return;
            if (!['ArrowUp', 'ArrowDown', 'Enter', 'Tab'].includes(e.key) || !open) track(e.currentTarget);
          },
          onClick: (e) => track(e.currentTarget),
          onBlur: () => setOpen(null),
        }}
      />
      {open && <LinkPickerList id={listId} search={search} index={index} onPick={insert} onFilter={() => setIndex(0)} />}
    </div>
  );
}
