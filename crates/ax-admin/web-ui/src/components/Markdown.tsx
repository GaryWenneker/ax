import { Fragment, type ReactNode } from 'react';
import { parseMarkdown, type Inline } from '../markdownModel';

function inline(parts: Inline[]): ReactNode {
  return parts.map((part, i) => {
    if (part.kind === 'code') return <code key={i}>{part.text}</code>;
    if (part.kind === 'strong') return <strong key={i}>{part.text}</strong>;
    return <Fragment key={i}>{part.text}</Fragment>;
  });
}

/** Renders tool text. Everything is a React text node; nothing is injected as HTML. */
export default function Markdown({ text }: { text: string }) {
  const blocks = parseMarkdown(text);
  return (
    <div className="admin-md">
      {blocks.map((block, i) => {
        switch (block.type) {
          case 'heading': {
            const Tag = `h${Math.min(block.level + 1, 4)}` as 'h2' | 'h3' | 'h4';
            return <Tag key={i}>{inline(block.inline)}</Tag>;
          }
          case 'code':
            return <pre key={i} tabIndex={0}><code>{block.text}</code></pre>;
          case 'list': {
            const Tag = block.ordered ? 'ol' : 'ul';
            return <Tag key={i}>{block.items.map((item, j) => <li key={j}>{inline(item)}</li>)}</Tag>;
          }
          case 'table':
            return (
              <div key={i} className="admin-table-wrap" tabIndex={0}>
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
          default:
            return <p key={i}>{inline(block.inline)}</p>;
        }
      })}
    </div>
  );
}
