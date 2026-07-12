import Codicon from './Codicon';
import {
  buildSymbolTree,
  sectionLabelForKind,
  shouldShowSectionHeader,
  type SymbolTreeNode,
} from '../lib/symbolTree';
import type { NodeRow } from '../types';

const KIND_ICONS: Record<string, string> = {
  function: 'symbol-method',
  method: 'symbol-method',
  class: 'symbol-class',
  struct: 'symbol-structure',
  enum: 'symbol-enum',
  trait: 'symbol-interface',
  interface: 'symbol-interface',
  type: 'symbol-type-parameter',
  const: 'symbol-constant',
  variable: 'symbol-variable',
  module: 'symbol-namespace',
  file: 'file',
};

function lineLabel(node: NodeRow): string {
  return node.start_line === node.end_line
    ? `:${node.start_line}`
    : `:${node.start_line}–${node.end_line}`;
}

function SymbolRow({
  entry,
  depth,
  isLast,
  ancestorLast,
  selectedId,
  onSelect,
}: {
  entry: SymbolTreeNode;
  depth: number;
  isLast: boolean;
  ancestorLast: boolean[];
  selectedId?: string | null;
  onSelect: (id: string) => void;
}) {
  const { node, children } = entry;
  const hasChildren = children.length > 0;

  return (
    <>
      <div className="symbol-tree-row-wrap">
        <div className="symbol-tree-guides" aria-hidden="true">
          {ancestorLast.map((last, i) => (
            <span
              key={i}
              className={`symbol-tree-guide${last ? ' symbol-tree-guide--blank' : ''}`}
            />
          ))}
          {depth > 0 && (
            <span
              className={`symbol-tree-guide symbol-tree-guide--joint${isLast ? ' symbol-tree-guide--last' : ''}`}
            />
          )}
        </div>
        <button
          type="button"
          className={`symbol-tree-row${hasChildren ? ' symbol-tree-row--parent' : ''}${selectedId === node.id ? ' symbol-tree-row--selected' : ''}`}
          onClick={() => onSelect(node.id)}
          title={node.qualified_name}
        >
          <Codicon
            name={KIND_ICONS[node.kind] ?? 'symbol-misc'}
            className="symbol-tree-icon"
          />
          <span className="symbol-tree-name">{node.name}</span>
          <span className="symbol-tree-lines">{lineLabel(node)}</span>
          <span className="symbol-tree-badges">
            <span className="page-item-badge">{node.kind}</span>
            {node.is_exported ? <span className="page-item-badge">pub</span> : null}
          </span>
          {node.signature && (
            <span className="symbol-tree-sig" title={node.signature}>
              {node.signature}
            </span>
          )}
        </button>
      </div>
      {children.map((child, i) => (
        <SymbolRow
          key={child.node.id}
          entry={child}
          depth={depth + 1}
          isLast={i === children.length - 1}
          ancestorLast={[...ancestorLast, isLast]}
          selectedId={selectedId}
          onSelect={onSelect}
        />
      ))}
    </>
  );
}

export default function SymbolOutline({
  nodes,
  selectedId,
  onSelect,
}: {
  nodes: NodeRow[];
  selectedId?: string | null;
  onSelect: (id: string) => void;
}) {
  const tree = buildSymbolTree(nodes);

  if (tree.length === 0) {
    return <div className="files-preview-empty-inline">No symbols indexed for this file.</div>;
  }

  return (
    <div className="symbol-tree">
      {tree.map((entry, i) => (
        <div key={entry.node.id} className="symbol-tree-section">
          {shouldShowSectionHeader(tree, i) && (
            <div className="symbol-tree-section-label">
              {sectionLabelForKind(entry.node.kind)}
            </div>
          )}
          <SymbolRow
            entry={entry}
            depth={0}
            isLast={i === tree.length - 1}
            ancestorLast={[]}
            selectedId={selectedId}
            onSelect={onSelect}
          />
        </div>
      ))}
    </div>
  );
}
