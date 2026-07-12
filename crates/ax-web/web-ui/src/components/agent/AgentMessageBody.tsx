interface Props {
  text: string;
  kind: 'user' | 'assistant' | 'system' | 'tool';
}

type Block =
  | { type: 'text'; content: string }
  | { type: 'code'; lang: string; content: string };

function parseBlocks(text: string): Block[] {
  const blocks: Block[] = [];
  const re = /```(\w*)\n?([\s\S]*?)```/g;
  let last = 0;
  let match: RegExpExecArray | null;
  while ((match = re.exec(text)) !== null) {
    if (match.index > last) {
      blocks.push({ type: 'text', content: text.slice(last, match.index) });
    }
    blocks.push({ type: 'code', lang: match[1] ?? '', content: match[2].replace(/\n$/, '') });
    last = match.index + match[0].length;
  }
  if (last < text.length) {
    blocks.push({ type: 'text', content: text.slice(last) });
  }
  return blocks.length ? blocks : [{ type: 'text', content: text }];
}

function renderInline(text: string) {
  const parts = text.split(/(`[^`]+`)/g);
  return parts.map((part, i) => {
    if (part.startsWith('`') && part.endsWith('`')) {
      return (
        <code key={i} className="agent-inline-code">
          {part.slice(1, -1)}
        </code>
      );
    }
    return <span key={i}>{part}</span>;
  });
}

export default function AgentMessageBody({ text, kind }: Props) {
  if (!text) return null;

  if (kind === 'tool') {
    const nl = text.indexOf('\n');
    const header = nl >= 0 ? text.slice(0, nl) : text;
    const body = nl >= 0 ? text.slice(nl + 1).trim() : '';
    return (
      <div className="agent-tool-msg">
        <div className="agent-tool-msg-header">{header}</div>
        {body && <pre className="agent-code-block">{body}</pre>}
      </div>
    );
  }

  if (kind === 'assistant') {
    const blocks = parseBlocks(text);
    return (
      <div className="agent-md">
        {blocks.map((block, i) => {
          if (block.type === 'code') {
            return (
              <div key={i} className="agent-code-wrap">
                {block.lang && <div className="agent-code-lang">{block.lang}</div>}
                <pre className="agent-code-block">{block.content}</pre>
              </div>
            );
          }
          if (!block.content.trim()) return null;
          return (
            <p key={i} className="agent-md-text">
              {renderInline(block.content)}
            </p>
          );
        })}
      </div>
    );
  }

  return <>{text}</>;
}
