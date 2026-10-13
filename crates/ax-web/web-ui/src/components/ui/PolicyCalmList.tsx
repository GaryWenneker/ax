import type { CSSProperties, MouseEvent } from 'react';
import { useTheme } from '../../hooks/useTheme';
import Codicon from '../Codicon';
import { OriginDbBadge, TagList } from '../PolicyMetaView';
import { ItemGroupHeader, ItemList, ItemRow, LevelBadge, ScopeBadge } from './PageLayout';
import { GitShareDot } from './GitShareDot';
import { normalizePolicyScope, policyDbAccent } from './policyListUtils';
import type {ColumnValue} from '../../lib/columnModel';

const SHOWN_TAGS = 3;

export interface PolicyCalmItem {
  key: string;
  id: string;
  subtitle: string;
  meta: string;
  level?: string;
  scope?: string;
  origin?: string;
  projectName?: string;
  tags: string[];
  enabled: boolean;
  global: boolean;
  storage: 'files' | 'database';
}

export interface PolicyCalmGroup {
  id: string;
  label: string;
  items: PolicyCalmItem[];
}

export interface PolicyCalmFilters {
  level: string;
  scope: string;
  tags: string[];
  onLevel?: (level: string) => void;
  onScope: (scope?: string) => void;
  onTag: (tag: string) => void;
}

interface Props {
  groups: PolicyCalmGroup[];
  collapsed: Set<string>;
  onToggleGroup: (id: string) => void;
  selectedIds: Set<string>;
  compact: boolean;
  filters: PolicyCalmFilters;
  onRowClick: (e: MouseEvent, id: string) => void;
  onRowContext: (e: MouseEvent, id: string) => void;
  onCheck: (id: string, checked: boolean, shiftKey?: boolean) => void;
  visibleSelected?: boolean;
  onToggleVisible?: () => void;
  onToggleEnabled: (id: string, enabled: boolean) => void;
  onToggleStorage: (id: string, current: 'files' | 'database') => void;
  onEdit: (id: string) => void;
  onDelete: (id: string) => void;
  /** Row keys that just appeared (live updates); they glow briefly. */
  fresh?: ReadonlySet<string>;
}

function isControl(e: MouseEvent) {
  return Boolean((e.target as HTMLElement).closest('button, a, input, select, textarea, label'));
}

export function PolicyCalmList(props: Props) {
  const portal = useTheme().layout === 'portal';
  return (
    <div className="calm-groups">
      {props.groups.map((g) => {
        const open = !props.collapsed.has(g.id);
        return (
          <section key={g.id} className="calm-group">
            <ItemGroupHeader label={g.label} count={g.items.length} open={open} onToggle={() => props.onToggleGroup(g.id)} />
            {open && (
              <ItemList className={`calm-list${portal ? ' portal-data-list portal-policy-list' : ''}`}>
                {portal && <div className="portal-data-header"><span data-column-label="Select" data-column-min={28}><input type="checkbox" aria-label="Select visible rows" checked={Boolean(props.visibleSelected)} onChange={() => props.onToggleVisible?.()} /></span><span>Name</span><span>Level</span><span>Scope</span><span>Priority</span><span>Enabled / actions</span></div>}
                {g.items.map((item) => (
                  <PolicyCalmRow key={item.key} item={item} {...props} listValues={['', item.id, item.level ?? '', normalizePolicyScope(item.scope), Number(item.meta.replace(/^p/, '')), item.enabled ? 'enabled' : 'disabled']} />
                ))}
              </ItemList>
            )}
          </section>
        );
      })}
    </div>
  );
}

function PolicyCalmRow({ item, listValues: _listValues, ...p }: Props & { item: PolicyCalmItem; listValues: readonly ColumnValue[] }) {
  const portal = useTheme().layout === 'portal';
  const selected = p.selectedIds.has(item.id);
  const shownTags = item.tags.slice(0, SHOWN_TAGS);
  const hiddenTags = item.tags.slice(SHOWN_TAGS);
  return (
    <ItemRow
      variant="graph"
      className={`policy-calm-row${item.global ? ' policy-calm-row--global' : ''}${p.fresh?.has(item.key) ? ' live-new' : ''}`}
      title={item.id}
      subtitle={item.subtitle}
      leading={
        <input
          type="checkbox"
          className={`calm-row-check${selected ? ' on' : ''}`}
          checked={selected}
          aria-label={`Select ${item.id}`}
          onClick={(e) => e.stopPropagation()}
          onChange={(e) => p.onCheck(item.id, e.target.checked, e.nativeEvent instanceof MouseEvent && e.nativeEvent.shiftKey)}
        />
      }
      meta={item.meta}
      portalColumns={portal ? [
        item.level ? <LevelBadge level={item.level} onClick={p.filters.onLevel ? () => p.filters.onLevel?.(item.level ?? '') : undefined} active={p.filters.level === item.level} /> : '—',
        <ScopeBadge scope={item.scope} onClick={() => p.filters.onScope(item.scope)} active={p.filters.scope === normalizePolicyScope(item.scope)} />,
        item.meta,
      ] : undefined}
      portalLabels={['Level', 'Scope', 'Priority']}
      metaTitle="Priority"
      selected={selected}
      disabled={!item.enabled}
      rowProps={{ role: 'group', 'aria-label': item.id }}
      style={{ '--node-color': policyDbAccent(item.origin) } as CSSProperties}
      onClick={(e) => {
        if (!isControl(e)) p.onRowClick(e, item.id);
      }}
      onContextMenu={(e) => p.onRowContext(e, item.id)}
      badges={
        p.compact ? (
          <GitShareDot scope={item.scope} enabled={item.enabled} />
        ) : (
          <>
            <GitShareDot scope={item.scope} enabled={item.enabled} />
            {item.level && (
              <LevelBadge
                level={item.level}
                onClick={p.filters.onLevel ? () => p.filters.onLevel?.(item.level ?? '') : undefined}
                active={p.filters.level === item.level}
              />
            )}
            <ScopeBadge
              scope={item.scope}
              onClick={() => p.filters.onScope(item.scope)}
              active={p.filters.scope === normalizePolicyScope(item.scope)}
            />
            {item.global && <OriginDbBadge origin={item.origin} projectName={item.projectName} />}
            {shownTags.length > 0 && (
              <TagList items={shownTags} onTagClick={p.filters.onTag} activeTags={p.filters.tags} />
            )}
            {hiddenTags.length > 0 && (
              <span className="page-item-badge" title={hiddenTags.join(', ')}>
                <Codicon name="git-branch" className="badge-icon" />+{hiddenTags.length}
              </span>
            )}
          </>
        )
      }
      aside={
        <div className="calm-row-aside">
          {!p.compact && (
            <>
              <button type="button" className="calm-row-action" onClick={() => p.onEdit(item.id)} aria-label={`Edit ${item.id}`} title="Edit">
                <Codicon name="edit" />
              </button>
              <button
                type="button"
                className="calm-row-action calm-row-action--remove"
                onClick={() => p.onDelete(item.id)}
                aria-label={`Delete ${item.id}`}
                title="Delete"
              >
                <Codicon name="trash" />
              </button>
              {item.global ? (
                <span className="calm-row-slot calm-row-slot--action" aria-hidden="true" />
              ) : (
                <button
                  type="button"
                  role="switch"
                  aria-checked={item.storage === 'database'}
                  className={`calm-row-action calm-row-storage${item.storage === 'database' ? ' on' : ''}`}
                  onClick={() => p.onToggleStorage(item.id, item.storage)}
                  aria-label={`Storage for ${item.id}`}
                  title={item.storage === 'database' ? 'Database — click for Files (MD)' : 'Files (MD) — click for Database'}
                >
                  {item.storage === 'database' ? 'DB' : 'MD'}
                </button>
              )}
            </>
          )}
          {item.global ? (
            <span className="calm-row-slot calm-row-slot--toggle" aria-hidden="true" />
          ) : (
            <button
              type="button"
              className={`settings-toggle${item.enabled ? ' on' : ''}`}
              onClick={() => p.onToggleEnabled(item.id, !item.enabled)}
              aria-pressed={item.enabled}
              aria-label={item.enabled ? `Disable ${item.id}` : `Enable ${item.id}`}
              title={item.enabled ? 'Enabled — click to disable' : 'Disabled — click to enable'}
            >
              <span className="settings-toggle-thumb" />
            </button>
          )}
        </div>
      }
    />
  );
}
