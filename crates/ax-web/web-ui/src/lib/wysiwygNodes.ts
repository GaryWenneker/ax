import { Node } from '@tiptap/core';

/** `[[target]]` / `[[target|alias]]`, kept verbatim so the resolver sees what was written. */
export const WikiLink = Node.create({
  name: 'wikilink',
  group: 'inline',
  inline: true,
  atom: true,
  selectable: true,
  addAttributes() {
    return { raw: { default: '' } };
  },
  parseHTML() {
    return [{ tag: 'span[data-wikilink]', getAttrs: (el) => ({ raw: (el as HTMLElement).dataset.wikilink ?? '' }) }];
  },
  renderHTML({ node }) {
    const raw = String(node.attrs.raw);
    const shown = raw.includes('|') ? raw.slice(raw.indexOf('|') + 1) : raw;
    return ['span', { class: 'wysiwyg-wikilink', 'data-wikilink': raw }, shown];
  },
  markdownTokenName: 'wikilink',
  markdownTokenizer: {
    name: 'wikilink',
    level: 'inline',
    start: (src: string) => src.indexOf('[['),
    tokenize: (src: string) => {
      const m = /^\[\[([^\]\n]+)\]\]/.exec(src);
      return m ? { type: 'wikilink', raw: m[0], text: m[1] } : undefined;
    },
  },
  parseMarkdown: (token) => ({ type: 'wikilink', attrs: { raw: token.text } }),
  renderMarkdown: (node) => `[[${node.attrs?.raw ?? ''}]]`,
});

/** HTML comments (`<!-- … -->`) are invisible in rendered Markdown but must survive a save. */
export const HtmlComment = Node.create({
  name: 'htmlComment',
  group: 'block',
  atom: true,
  addAttributes() {
    return { raw: { default: '' } };
  },
  parseHTML() {
    return [{ tag: 'div[data-html-comment]', getAttrs: (el) => ({ raw: (el as HTMLElement).dataset.htmlComment ?? '' }) }];
  },
  renderHTML({ node }) {
    return ['div', { class: 'wysiwyg-comment', 'data-html-comment': node.attrs.raw }, String(node.attrs.raw)];
  },
  markdownTokenName: 'htmlComment',
  markdownTokenizer: {
    name: 'htmlComment',
    level: 'block',
    start: (src: string) => src.indexOf('<!--'),
    tokenize: (src: string) => {
      const m = /^<!--[\s\S]*?-->[ \t]*(?:\n|$)/.exec(src);
      return m ? { type: 'htmlComment', raw: m[0], text: m[0].trimEnd() } : undefined;
    },
  },
  parseMarkdown: (token) => ({ type: 'htmlComment', attrs: { raw: token.text } }),
  renderMarkdown: (node) => String(node.attrs?.raw ?? ''),
});
