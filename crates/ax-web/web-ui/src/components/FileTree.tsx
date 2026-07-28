import { useEffect, useMemo, useRef, useState } from 'react';
import type { FileRoot, FileRow } from '../types';
import { Spinner } from './ui/Spinner';
import Codicon from './Codicon';
import FileTypeIcon from './FileTypeIcon';
import { loadJson, saveJson } from '../lib/uiStorage';

export interface TreeNode {
  name: string;
  path: string;
  kind: 'folder' | 'file';
  children?: TreeNode[];
  file?: FileRow;
  /** Total indexed files under this folder (from API), when known. */
  folderCount?: number;
}

// v2: browse mode no longer auto-expands lazy root folders (old key left stale "all open").
const EXPANDED_KEY = 'files-tree-expanded-v2';

function sortTree(node: TreeNode) {
  if (!node.children) return;
  node.children.sort((a, b) => {
    if (a.kind !== b.kind) return a.kind === 'folder' ? -1 : 1;
    return a.name.localeCompare(b.name, undefined, { sensitivity: 'base' });
  });
  node.children.forEach(sortTree);
}

export function buildFileTree(files: FileRow[]): TreeNode {
  const root: TreeNode = { name: '', path: '', kind: 'folder', children: [] };

  for (const file of files) {
    const parts = file.path.split('/').filter(Boolean);
    let node = root;

    for (let i = 0; i < parts.length; i++) {
      const isFile = i === parts.length - 1;
      const name = parts[i];
      const path = parts.slice(0, i + 1).join('/');

      if (isFile) {
        node.children!.push({ name, path, kind: 'file', file });
      } else {
        let child = node.children!.find((c) => c.kind === 'folder' && c.name === name);
        if (!child) {
          child = { name, path, kind: 'folder', children: [] };
          node.children!.push(child);
        }
        node = child;
      }
    }
  }

  sortTree(root);
  return root;
}

function mergeRootsIntoTree(tree: TreeNode, roots: FileRoot[]): TreeNode {
  if (!roots.length) return tree;
  if (!tree.children) tree.children = [];

  const existing = new Set(
    tree.children.filter((c) => c.kind === 'folder').map((c) => c.path),
  );

  for (const root of roots) {
    if (existing.has(root.path)) continue;
    tree.children.push({
      name: root.name,
      path: root.path,
      kind: 'folder',
      children: [],
      folderCount: root.count,
    });
  }

  sortTree(tree);
  return tree;
}

function folderPathsToExpand(paths: string[], filterActive: boolean): Set<string> {
  const expanded = new Set<string>();
  if (!filterActive) return expanded;
  for (const path of paths) {
    const parts = path.split('/').filter(Boolean);
    let current = '';
    for (let i = 0; i < parts.length - 1; i++) {
      current = current ? `${current}/${parts[i]}` : parts[i];
      expanded.add(current);
    }
  }
  return expanded;
}

function loadExpandedSet(): Set<string> | null {
  const stored = loadJson<string[] | null>(EXPANDED_KEY, null);
  if (stored && Array.isArray(stored)) return new Set(stored);
  return null;
}

function formatBytes(b: number) {
  if (b < 1024) return `${b} B`;
  if (b < 1024 * 1024) return `${(b / 1024).toFixed(1)} KB`;
  return `${(b / (1024 * 1024)).toFixed(1)} MB`;
}

function countFiles(node: TreeNode): number {
  if (node.kind === 'file') return 1;
  return (node.children ?? []).reduce((sum, c) => sum + countFiles(c), 0);
}

function TreeRow({
  node,
  depth,
  expanded,
  selectedPath,
  loadingPrefixes,
  onToggle,
  onSelect,
}: {
  node: TreeNode;
  depth: number;
  expanded: Set<string>;
  selectedPath: string | null;
  loadingPrefixes?: Set<string>;
  onToggle: (path: string) => void;
  onSelect: (file: FileRow) => void;
}) {
  const isOpen = expanded.has(node.path);
  const fileCount =
    node.kind === 'folder'
      ? (node.folderCount ?? countFiles(node))
      : 0;
  const isLoading = node.kind === 'folder' && loadingPrefixes?.has(node.path);

  if (node.kind === 'file' && node.file) {
    const f = node.file;
    const selected = selectedPath === f.path;
    return (
      <button
        type="button"
        className={`file-tree-row file-tree-row--file${selected ? ' selected' : ''}`}
        style={{ paddingLeft: `${8 + depth * 16}px` }}
        onClick={() => onSelect(f)}
        title={f.path}
      >
        <span className="file-tree-spacer" aria-hidden="true" />
        <FileTypeIcon
          fileName={node.name}
          language={f.language}
          className="file-tree-icon file-tree-icon--file"
        />
        <span className="file-tree-name">{node.name}</span>
        <span className="file-tree-meta">
          <span className="file-tree-meta-item">{f.node_count} nodes</span>
          <span className="file-tree-meta-item">{formatBytes(f.size)}</span>
          <span className="page-item-badge">{f.language}</span>
        </span>
      </button>
    );
  }

  return (
    <>
      <button
        type="button"
        className={`file-tree-row file-tree-row--folder${isLoading ? ' file-tree-row--loading' : ''}`}
        style={{ paddingLeft: `${8 + depth * 16}px` }}
        onClick={() => onToggle(node.path)}
        aria-expanded={isOpen}
      >
        <Codicon
          name={isOpen ? 'chevron-down' : 'chevron-right'}
          className="file-tree-chevron"
        />
        <FileTypeIcon
          fileName={node.name}
          kind="folder"
          open={isOpen}
          className="file-tree-icon file-tree-icon--folder"
        />
        <span className="file-tree-name">{node.name}</span>
        <span className="file-tree-folder-count">
          {isLoading ? <Spinner size="xs" /> : fileCount}
        </span>
      </button>
      {isOpen &&
        node.children?.map((child) => (
          <TreeRow
            key={child.path}
            node={child}
            depth={depth + 1}
            expanded={expanded}
            selectedPath={selectedPath}
            loadingPrefixes={loadingPrefixes}
            onToggle={onToggle}
            onSelect={onSelect}
          />
        ))}
    </>
  );
}

export default function FileTree({
  files,
  roots,
  rootLabel,
  filterActive,
  selectedPath,
  loadingPrefixes,
  onSelect,
  onLoadPrefix,
  onRefresh,
}: {
  files: FileRow[];
  roots?: FileRoot[];
  rootLabel?: string;
  filterActive?: boolean;
  selectedPath?: string | null;
  loadingPrefixes?: Set<string>;
  onSelect?: (file: FileRow) => void;
  onLoadPrefix?: (prefix: string) => void;
  onRefresh?: () => void;
}) {
  const tree = useMemo(() => {
    const built = buildFileTree(files);
    return roots?.length ? mergeRootsIntoTree(built, roots) : built;
  }, [files, roots]);
  const hydrated = useRef(false);

  // Filter mode: expand ancestors of matches. Browse mode: start collapsed —
  // top-level repos are lazy shells until the user opens them.
  const defaultExpanded = useMemo(
    () =>
      folderPathsToExpand(
        files.map((f) => f.path),
        !!filterActive,
      ),
    [files, filterActive],
  );

  const [expanded, setExpanded] = useState<Set<string>>(() => {
    if (filterActive) return defaultExpanded;
    const stored = loadExpandedSet();
    if (stored && stored.size > 0) return stored;
    return new Set();
  });

  useEffect(() => {
    if (!hydrated.current) {
      hydrated.current = true;
      if (!filterActive) {
        const stored = loadExpandedSet();
        if (stored && stored.size > 0) {
          setExpanded(stored);
          return;
        }
        setExpanded(new Set());
        return;
      }
    }
    if (filterActive) {
      setExpanded((prev) => {
        const next = new Set(prev);
        defaultExpanded.forEach((p) => next.add(p));
        saveJson(EXPANDED_KEY, [...next]);
        return next;
      });
    }
  }, [defaultExpanded, filterActive]);

  // Persisted / restored expanded paths must still fetch their lazy children.
  useEffect(() => {
    if (!onLoadPrefix || filterActive) return;
    for (const path of expanded) {
      if (path) onLoadPrefix(path);
    }
  }, [expanded, onLoadPrefix, filterActive]);

  function toggle(path: string) {
    const willOpen = !expanded.has(path);
    if (willOpen) {
      onLoadPrefix?.(path);
    }
    setExpanded((prev) => {
      const next = new Set(prev);
      if (next.has(path)) next.delete(path);
      else next.add(path);
      saveJson(EXPANDED_KEY, [...next]);
      return next;
    });
  }

  function collapseAll() {
    setExpanded(new Set());
    saveJson(EXPANDED_KEY, []);
  }

  function select(file: FileRow) {
    onSelect?.(file);
  }

  const children = tree.children ?? [];

  return (
    <div className="file-tree">
      {rootLabel && (
        <div className="file-tree-root">
          <Codicon name="repo" className="file-tree-root-icon" />
          <span className="file-tree-root-label">{rootLabel}</span>
          <div className="file-tree-root-actions">
            <button
              type="button"
              className="file-tree-root-btn"
              onClick={collapseAll}
              title="Collapse all folders"
              aria-label="Collapse all folders"
            >
              <Codicon name="collapse-all" />
            </button>
            {onRefresh && (
              <button
                type="button"
                className="file-tree-root-btn"
                onClick={onRefresh}
                title="Refresh explorer"
                aria-label="Refresh explorer"
              >
                <Codicon name="refresh" />
              </button>
            )}
          </div>
        </div>
      )}
      {children.map((child) => (
        <TreeRow
          key={child.path}
          node={child}
          depth={rootLabel ? 1 : 0}
          expanded={expanded}
          selectedPath={selectedPath ?? null}
          loadingPrefixes={loadingPrefixes}
          onToggle={toggle}
          onSelect={select}
        />
      ))}
    </div>
  );
}
