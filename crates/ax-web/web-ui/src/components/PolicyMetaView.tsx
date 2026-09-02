import { LevelBadge, ScopeBadge } from './ui/PageLayout';
import { GitShareDot } from './ui/GitShareDot';
import { isGitShared } from './ui/policyListUtils';
import { scopeLabel } from '../policyTypes';
import { skillGroupLabel } from '../skillGroups';

function GitShareStatus({ scope, enabled }: { scope?: string; enabled?: boolean }) {
  if (isGitShared(scope, enabled)) {
    return (
      <span className="policy-git-share-status">
        <GitShareDot scope={scope} enabled={enabled} />
        Shared via git (.agents)
      </span>
    );
  }
  return <span>Not git-shared</span>;
}

export function TagList({
  items,
  empty = '—',
  onTagClick,
  activeTags,
}: {
  items: string[];
  empty?: string;
  onTagClick?: (tag: string) => void;
  /** Tags currently active in a filter — highlighted when present. */
  activeTags?: string[];
}) {
  if (items.length === 0) {
    return <span className="policy-view-empty">{empty}</span>;
  }
  const active = new Set((activeTags ?? []).map((t) => t.toLowerCase()));
  const allTitle = items.join(', ');
  return (
    <span className="policy-view-tags" title={allTitle}>
      {items.map((item) => {
        const isActive = active.has(item.toLowerCase());
        if (onTagClick) {
          return (
            <button
              key={item}
              type="button"
              className={`page-item-badge page-item-badge--btn${isActive ? ' page-item-badge--active' : ''}`}
              onClick={(e) => {
                e.stopPropagation();
                onTagClick(item);
              }}
              title={isActive ? `Remove filter: ${item}` : `Filter by ${item}`}
            >
              {item}
            </button>
          );
        }
        return (
          <span key={item} className={`page-item-badge${isActive ? ' page-item-badge--active' : ''}`}>
            {item}
          </span>
        );
      })}
    </span>
  );
}

export function RuleMetaView({
  id,
  level,
  alwaysApply,
  priority,
  globs,
  triggers,
  tags,
  scope,
  enabled = true,
  group,
  onLevelClick,
  onScopeClick,
  onTagClick,
  activeLevel,
  activeScope,
  activeTags,
}: {
  id: string;
  level: string;
  alwaysApply: boolean;
  priority: number;
  globs: string[];
  triggers: string[];
  tags: string[];
  scope?: string;
  enabled?: boolean;
  group?: string | null;
  onLevelClick?: (level: string) => void;
  onScopeClick?: (scope?: string) => void;
  onTagClick?: (tag: string) => void;
  activeLevel?: string;
  activeScope?: string;
  activeTags?: string[];
}) {
  return (
    <div className="policy-view-meta">
      <div className="detail-kv">
        <span className="detail-key">ID</span>
        <span className="detail-val">{id || '—'}</span>
      </div>
      <div className="detail-kv">
        <span className="detail-key">Enabled</span>
        <span className="detail-val">{enabled !== false ? 'On — active in matching' : 'Off — skipped by preflight'}</span>
      </div>
      <div className="detail-kv">
        <span className="detail-key">Level</span>
        <span className="detail-val">
          <LevelBadge
            level={level}
            onClick={onLevelClick ? () => onLevelClick(level) : undefined}
            active={activeLevel === level}
          />
        </span>
      </div>
      <div className="detail-kv">
        <span className="detail-key">Scope</span>
        <span className="detail-val" title={scopeLabel(scope)}>
          <ScopeBadge
            scope={scope}
            onClick={onScopeClick ? () => onScopeClick(scope) : undefined}
            active={activeScope === (scope || 'project').toLowerCase()}
          />
        </span>
      </div>
      <div className="detail-kv">
        <span className="detail-key">Git</span>
        <span className="detail-val">
          <GitShareStatus scope={scope} enabled={enabled} />
        </span>
      </div>
      <div className="detail-kv">
        <span className="detail-key">Group</span>
        <span className="detail-val">{skillGroupLabel(group || 'ungrouped')}</span>
      </div>
      <div className="detail-kv">
        <span className="detail-key">Always apply</span>
        <span className="detail-val">{alwaysApply ? 'Yes — every agent turn' : 'No — match triggers/globs'}</span>
      </div>
      <div className="detail-kv">
        <span className="detail-key">Priority</span>
        <span className="detail-val">{priority}</span>
      </div>
      <div className="detail-kv detail-kv--stack">
        <span className="detail-key">Globs</span>
        <span className="detail-val"><TagList items={globs} empty="Any file" /></span>
      </div>
      <div className="detail-kv detail-kv--stack">
        <span className="detail-key">Triggers</span>
        <span className="detail-val"><TagList items={triggers} empty="None" /></span>
      </div>
      <div className="detail-kv detail-kv--stack">
        <span className="detail-key">Tags</span>
        <span className="detail-val">
          <TagList items={tags} empty="None" onTagClick={onTagClick} activeTags={activeTags} />
        </span>
      </div>
    </div>
  );
}

export function SkillMetaView({
  name,
  description,
  alwaysApply = false,
  priority,
  triggers,
  tags,
  contextTask,
  scope,
  enabled = true,
  group,
}: {
  name: string;
  description: string;
  alwaysApply?: boolean;
  priority: number;
  triggers: string[];
  tags: string[];
  contextTask?: string;
  scope?: string;
  enabled?: boolean;
  group?: string | null;
}) {
  return (
    <div className="policy-view-meta">
      <div className="detail-kv">
        <span className="detail-key">Name</span>
        <span className="detail-val">{name || '—'}</span>
      </div>
      <div className="detail-kv">
        <span className="detail-key">Enabled</span>
        <span className="detail-val">{enabled !== false ? 'On — active in matching' : 'Off — skipped by preflight'}</span>
      </div>
      <div className="detail-kv">
        <span className="detail-key">Always apply</span>
        <span className="detail-val">{alwaysApply ? 'Yes — every agent turn, including empty prompts' : 'No — match triggers/description'}</span>
      </div>
      <div className="detail-kv detail-kv--stack">
        <span className="detail-key">Description</span>
        <span className="detail-val">{description || '—'}</span>
      </div>
      <div className="detail-kv">
        <span className="detail-key">Scope</span>
        <span className="detail-val" title={scopeLabel(scope)}>
          <ScopeBadge scope={scope} />
        </span>
      </div>
      <div className="detail-kv">
        <span className="detail-key">Git</span>
        <span className="detail-val">
          <GitShareStatus scope={scope} enabled={enabled} />
        </span>
      </div>
      <div className="detail-kv">
        <span className="detail-key">Group</span>
        <span className="detail-val">{skillGroupLabel(group || 'ungrouped')}</span>
      </div>
      <div className="detail-kv">
        <span className="detail-key">Priority</span>
        <span className="detail-val">{priority}</span>
      </div>
      <div className="detail-kv detail-kv--stack">
        <span className="detail-key">Triggers</span>
        <span className="detail-val"><TagList items={triggers} empty="None" /></span>
      </div>
      <div className="detail-kv detail-kv--stack">
        <span className="detail-key">Tags</span>
        <span className="detail-val"><TagList items={tags} empty="None" /></span>
      </div>
      {contextTask && (
        <div className="detail-kv detail-kv--stack">
          <span className="detail-key">Context task</span>
          <span className="detail-val">{contextTask}</span>
        </div>
      )}
    </div>
  );
}
