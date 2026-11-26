import { Fragment, memo, useMemo, type ReactNode } from 'react';
import { parseMarkdown, type Block, type Inline } from '../markdownModel';

function inline(parts: Inline[]): ReactNode {
  return parts.map((part, i) => {
    if (part.kind === 'code') return <code key={i}>{part.text}</code>;
    if (part.kind === 'strong') return <strong key={i}>{part.text}</strong>;
    return <Fragment key={i}>{part.text}</Fragment>;
  });
}

function BlockView({ block }: { block: Block }) {
  switch (block.type) {
    case 'heading': {
      const Tag = block.level <= 1 ? 'h2' : block.level === 2 ? 'h3' : 'h4';
      return <Tag>{inline(block.inline)}</Tag>;
    }
    case 'code':
      return <pre tabIndex={0}><code>{block.text}</code></pre>;
    case 'list': {
      const Tag = block.ordered ? 'ol' : 'ul';
      return <Tag>{block.items.map((item, j) => <li key={j}>{inline(item)}</li>)}</Tag>;
    }
    case 'table':
      return (
        <div className="admin-table-wrap" tabIndex={0}>
          <table className="admin-table">
            <thead><tr>{block.header.map((cell, j) => <th key={j} scope="col">{inline(cell)}</th>)}</tr></thead>
            <tbody>
              {block.rows.map((row, j) => (
                <tr key={j}>{row.map((cell, k) => <td key={k}>{inline(cell)}</td>)}</tr>
              ))}
            </tbody>
          </table>
        </div>
      );
    case 'text':
      return <p>{inline(block.inline)}</p>;
    default: {
      const unknown: never = block;
      return unknown;
    }
  }
}

/** Renders tool text. Everything is a React text node; nothing is injected as HTML. */
function Markdown({ text }: { text: string }) {
  const blocks = useMemo(() => parseMarkdown(text), [text]);
  return <div className="admin-md">{blocks.map((block, i) => <BlockView key={i} block={block} />)}</div>;
}

export default memo(Markdown);
