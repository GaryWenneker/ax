import { LevelBadge } from './ui/PageLayout';

function TagList({ items, empty = '—' }: { items: string[]; empty?: string }) {
  if (items.length === 0) {
    return <span className="policy-view-empty">{empty}</span>;
  }
  return (
    <span className="policy-view-tags">
      {items.map((item) => (
        <span key={item} className="page-item-badge">{item}</span>
      ))}
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
}: {
  id: string;
  level: string;
  alwaysApply: boolean;
  priority: number;
  globs: string[];
  triggers: string[];
  tags: string[];
}) {
  return (
    <div className="policy-view-meta">
      <div className="detail-kv">
        <span className="detail-key">ID</span>
        <span className="detail-val">{id || '—'}</span>
      </div>
      <div className="detail-kv">
        <span className="detail-key">Level</span>
        <span className="detail-val"><LevelBadge level={level} /></span>
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
        <span className="detail-val"><TagList items={tags} empty="None" /></span>
      </div>
    </div>
  );
}

export function SkillMetaView({
  name,
  description,
  priority,
  triggers,
  tags,
  contextTask,
}: {
  name: string;
  description: string;
  priority: number;
  triggers: string[];
  tags: string[];
  contextTask?: string;
}) {
  return (
    <div className="policy-view-meta">
      <div className="detail-kv">
        <span className="detail-key">Name</span>
        <span className="detail-val">{name || '—'}</span>
      </div>
      <div className="detail-kv detail-kv--stack">
        <span className="detail-key">Description</span>
        <span className="detail-val">{description || '—'}</span>
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
