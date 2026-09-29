import { useEffect, useId, useLayoutEffect, useRef, useState } from 'react';
import { EditorContent, useEditor, useEditorState, type Editor } from '@tiptap/react';
import { Extension, type Range } from '@tiptap/core';
import StarterKit from '@tiptap/starter-kit';
import { TableKit } from '@tiptap/extension-table';
import { TaskItem, TaskList } from '@tiptap/extension-list';
import { Markdown } from '@tiptap/markdown';
import Suggestion, { type SuggestionProps } from '@tiptap/suggestion';
import Codicon from './Codicon';
import LinkPickerList, { optionId, stepIndex, useLinkSearch, type PickerRow } from './LinkPicker';
import { wikiRaw } from '../lib/linkPicker';
import { HtmlComment, WikiLink } from '../lib/wysiwygNodes';

/** Offset inside `.wysiwyg-editor`; `position: fixed` would follow the blade's transform instead of the viewport. */
type Anchor = { left: number; top: number };

/** `inline`: typed `[[`, the query lives in the document. `button`: toolbar or ⌘K, with its own search box. */
type PickerState =
  | { mode: 'inline'; query: string; range: Range; anchor: Anchor | null }
  | { mode: 'button'; query: string; from: number; to: number; anchor: Anchor };

interface PickerBridge {
  openInline: (p: { query: string; range: Range; anchor: Anchor | null }) => void;
  closeInline: () => void;
  key: (e: KeyboardEvent) => boolean;
  openButton: (anchor?: Anchor) => void;
}

function linkPickerExtension(bridge: React.RefObject<PickerBridge | null>) {
  const toState = (p: SuggestionProps) => {
    const r = p.clientRect?.();
    return { query: p.query, range: p.range, anchor: r ? { left: r.left, top: r.bottom + 4 } : null };
  };
  return Extension.create({
    name: 'wikilinkPicker',
    addKeyboardShortcuts() {
      return {
        'Mod-k': () => {
          bridge.current?.openButton();
          return true;
        },
      };
    },
    addProseMirrorPlugins() {
      return [
        Suggestion({
          editor: this.editor,
          char: '[[',
          allowSpaces: true,
          items: () => [],
          render: () => ({
            onStart: (p) => bridge.current?.openInline(toState(p)),
            onUpdate: (p) => bridge.current?.openInline(toState(p)),
            onKeyDown: ({ event }) => bridge.current?.key(event) ?? false,
            onExit: () => bridge.current?.closeInline(),
          }),
        }),
      ];
    },
  });
}

type Tool = { id: string; label: string; icon?: string; text?: string; run: (e: Editor) => void; active: (e: Editor) => boolean };

const TOOLS: Tool[] = [
  { id: 'bold', icon: 'bold', label: 'Bold (⌘B)', run: (e) => e.chain().focus().toggleBold().run(), active: (e) => e.isActive('bold') },
  { id: 'italic', icon: 'italic', label: 'Italic (⌘I)', run: (e) => e.chain().focus().toggleItalic().run(), active: (e) => e.isActive('italic') },
  { id: 'strike', text: 'S', label: 'Strikethrough', run: (e) => e.chain().focus().toggleStrike().run(), active: (e) => e.isActive('strike') },
  { id: 'code', icon: 'code', label: 'Inline code', run: (e) => e.chain().focus().toggleCode().run(), active: (e) => e.isActive('code') },
  { id: 'h1', text: 'H1', label: 'Heading 1', run: (e) => e.chain().focus().toggleHeading({ level: 1 }).run(), active: (e) => e.isActive('heading', { level: 1 }) },
  { id: 'h2', text: 'H2', label: 'Heading 2', run: (e) => e.chain().focus().toggleHeading({ level: 2 }).run(), active: (e) => e.isActive('heading', { level: 2 }) },
  { id: 'h3', text: 'H3', label: 'Heading 3', run: (e) => e.chain().focus().toggleHeading({ level: 3 }).run(), active: (e) => e.isActive('heading', { level: 3 }) },
  { id: 'ul', icon: 'list-unordered', label: 'Bullet list', run: (e) => e.chain().focus().toggleBulletList().run(), active: (e) => e.isActive('bulletList') },
  { id: 'ol', icon: 'list-ordered', label: 'Numbered list', run: (e) => e.chain().focus().toggleOrderedList().run(), active: (e) => e.isActive('orderedList') },
  { id: 'task', icon: 'checklist', label: 'Task list', run: (e) => e.chain().focus().toggleTaskList().run(), active: (e) => e.isActive('taskList') },
  { id: 'quote', icon: 'quote', label: 'Quote', run: (e) => e.chain().focus().toggleBlockquote().run(), active: (e) => e.isActive('blockquote') },
  { id: 'block', icon: 'file-code', label: 'Code block', run: (e) => e.chain().focus().toggleCodeBlock().run(), active: (e) => e.isActive('codeBlock') },
];

const LINK_ACTIVE = (e: Editor) => e.isActive('link') || e.isActive('wikilink');

function Toolbar({ editor, onLink }: { editor: Editor; onLink: (anchor: Anchor) => void }) {
  const active = useEditorState({ editor, selector: ({ editor: e }) => [...TOOLS.map((t) => t.active(e)), LINK_ACTIVE(e)] });
  const linkOn = active[TOOLS.length];
  return (
    <div className="wysiwyg-toolbar" role="toolbar" aria-label="Formatting">
      {TOOLS.map((t, i) => (
        <button
          key={t.id}
          type="button"
          className={`btn wysiwyg-tool${active[i] ? ' wysiwyg-tool--active' : ''}`}
          aria-label={t.label}
          aria-pressed={active[i]}
          title={t.label}
          onMouseDown={(e) => e.preventDefault()}
          onClick={() => t.run(editor)}
        >
          {t.icon ? <Codicon name={t.icon} /> : <span className={`wysiwyg-tool-text wysiwyg-tool-text--${t.id}`}>{t.text}</span>}
        </button>
      ))}
      <button
        type="button"
        className={`btn wysiwyg-tool${linkOn ? ' wysiwyg-tool--active' : ''}`}
        aria-label="Link to a rule, skill or memory (⌘K)"
        aria-haspopup="dialog"
        title="Link to a rule, skill or memory (⌘K)"
        onMouseDown={(e) => e.preventDefault()}
        onClick={(e) => {
          const r = e.currentTarget.getBoundingClientRect();
          onLink({ left: r.left, top: r.bottom + 4 });
        }}
      >
        <Codicon name="link" />
      </button>
    </div>
  );
}

/**
 * Rich-text editing of a Markdown body; emits Markdown only after a real edit.
 * `value` is read once at mount: callers remount it (via `key` or by unmounting while loading) for a new body.
 */
export default function WysiwygEditor({
  value,
  onChange,
  selfKey,
}: {
  value: string;
  onChange: (value: string) => void;
  selfKey?: string;
}) {
  const listId = useId();
  const [picker, setPicker] = useState<PickerState | null>(null);
  const [index, setIndex] = useState(0);
  const search = useLinkSearch(picker !== null, picker?.query ?? '', selfKey, picker?.mode === 'button');
  const rows = search.rows;
  const bridge = useRef<PickerBridge | null>(null);
  const onChangeRef = useRef(onChange);
  const panelRef = useRef<HTMLDivElement | null>(null);
  const inputRef = useRef<HTMLInputElement | null>(null);
  const rootRef = useRef<HTMLDivElement | null>(null);
  const local = (a: Anchor): Anchor => {
    const r = rootRef.current?.getBoundingClientRect();
    return r ? { left: a.left - r.left, top: a.top - r.top } : a;
  };

  const editor = useEditor({
    extensions: [
      StarterKit.configure({ link: { openOnClick: false, autolink: true } }),
      TableKit,
      TaskList,
      TaskItem.configure({ nested: true }),
      Markdown,
      WikiLink,
      HtmlComment,
      linkPickerExtension(bridge),
    ],
    content: value,
    contentType: 'markdown',
    editorProps: {
      attributes: { class: 'wysiwyg-content', 'aria-label': 'Body (WYSIWYG)', spellcheck: 'false', 'aria-autocomplete': 'list' },
    },
    onUpdate: ({ editor: e }) => onChangeRef.current(e.getMarkdown()),
  });

  const closeButton = () => {
    setPicker(null);
    editor?.commands.focus();
  };

  const insert = (row: PickerRow) => {
    if (!editor || !picker) return;
    if (picker.mode === 'inline') {
      if (row.type === 'target') {
        editor.chain().focus().deleteRange(picker.range).insertContent({ type: 'wikilink', attrs: { raw: row.item.target } }).run();
      }
      setPicker(null);
      return;
    }
    const { from, to } = picker;
    const chain = editor.chain().focus().setTextSelection({ from, to });
    if (row.type === 'url') {
      if (from < to) chain.setLink({ href: row.url }).run();
      else chain.insertContent({ type: 'text', text: row.url, marks: [{ type: 'link', attrs: { href: row.url } }] }).run();
    } else {
      const label = editor.state.doc.textBetween(from, to, ' ');
      chain.insertContent({ type: 'wikilink', attrs: { raw: wikiRaw(row.item, label) } }).run();
    }
    setPicker(null);
  };

  const pickerKey = (key: string): boolean => {
    const next = stepIndex(key, index, rows.length);
    if (next !== null) {
      setIndex(next);
      return true;
    }
    if (key === 'Enter' && rows[index]) {
      insert(rows[index]);
      return true;
    }
    return false;
  };

  useLayoutEffect(() => {
    onChangeRef.current = onChange;
    bridge.current = {
      openInline: (p) => {
        if (picker?.mode === 'button') return;
        if (p.query !== picker?.query) setIndex(0);
        setPicker({ mode: 'inline', ...p, anchor: p.anchor && local(p.anchor) });
      },
      closeInline: () => setPicker((cur) => (cur?.mode === 'inline' ? null : cur)),
      key: (e) => {
        if (picker?.mode !== 'inline') return false;
        if (e.key === 'Escape') {
          setPicker(null);
          return true;
        }
        if (e.key === 'Tab' && rows[index]) {
          insert(rows[index]);
          return true;
        }
        return pickerKey(e.key);
      },
      openButton: (anchor) => {
        if (!editor) return;
        const { from, to } = editor.state.selection;
        const at = anchor ?? (() => {
          const c = editor.view.coordsAtPos(from);
          return { left: c.left, top: c.bottom + 4 };
        })();
        setIndex(0);
        setPicker({ mode: 'button', query: '', from, to, anchor: local(at) });
      },
    };
    const dom = editor?.view.dom;
    if (!dom) return;
    const inline = picker?.mode === 'inline';
    dom.setAttribute('aria-expanded', String(inline));
    if (inline) dom.setAttribute('aria-controls', listId);
    else dom.removeAttribute('aria-controls');
    if (inline && rows[index]) dom.setAttribute('aria-activedescendant', optionId(listId, index));
    else dom.removeAttribute('aria-activedescendant');
  });

  const buttonOpen = picker?.mode === 'button';
  useEffect(() => {
    if (!buttonOpen) return;
    inputRef.current?.focus();
    const onDown = (e: MouseEvent) => {
      if (e.target instanceof Node && panelRef.current?.contains(e.target)) return;
      setPicker(null);
    };
    document.addEventListener('mousedown', onDown);
    return () => document.removeEventListener('mousedown', onDown);
  }, [buttonOpen]);

  if (!editor) return null;
  const anchor = picker?.anchor;
  const style: React.CSSProperties | undefined = anchor ? { position: 'absolute', left: anchor.left, top: anchor.top, bottom: 'auto' } : undefined;
  return (
    <div className="wysiwyg-editor" ref={rootRef}>
      <Toolbar editor={editor} onLink={(a) => bridge.current?.openButton(a)} />
      <EditorContent editor={editor} className="wysiwyg-scroll" />
      {picker && (
        <LinkPickerList
          id={listId}
          search={search}
          index={index}
          onPick={insert}
          onFilter={() => setIndex(0)}
          style={style}
          dialogLabel={picker.mode === 'button' ? 'Insert link' : undefined}
          panelRef={panelRef}
          searchBox={
            picker.mode === 'button' && (
              <input
                ref={inputRef}
                type="search"
                className="link-picker-search"
                placeholder="Search · #tag · s: r: m: · or paste a URL"
                aria-label="Search links"
                aria-controls={listId}
                aria-activedescendant={rows[index] ? optionId(listId, index) : undefined}
                value={picker.query}
                onChange={(e) => {
                  const query = e.target.value;
                  setIndex(0);
                  setPicker((cur) => (cur?.mode === 'button' ? { ...cur, query } : cur));
                }}
                onKeyDown={(e) => {
                  if (e.key === 'Escape') {
                    e.preventDefault();
                    e.stopPropagation();
                    closeButton();
                    return;
                  }
                  if (pickerKey(e.key)) e.preventDefault();
                }}
              />
            )
          }
        />
      )}
    </div>
  );
}
