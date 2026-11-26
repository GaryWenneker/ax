import Codicon from '@cc/components/Codicon';

/** Square outline chip with a leading icon, as on every Command Center page. */
export default function Badge({ icon, label, tone }: { icon: string; label: string; tone?: 'warn' | 'danger' | 'ok' }) {
  return (
    <span className={`admin-badge${tone ? ` admin-badge--${tone}` : ''}`}>
      <Codicon name={icon} className="badge-icon" />
      {label}
    </span>
  );
}
