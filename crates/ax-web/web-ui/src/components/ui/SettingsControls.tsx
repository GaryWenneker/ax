import type { ReactNode } from 'react';

/** One labelled row of a settings card: title, optional description and lock note, then the control. */
export function SettingRow({
  title,
  description,
  locked,
  children,
}: {
  title: ReactNode;
  description?: string;
  locked?: string;
  children: ReactNode;
}) {
  return (
    <div className="settings-row">
      <div className="settings-row-label">
        <span className="settings-row-title">{title}</span>
        {description && <span className="settings-row-desc">{description}</span>}
        {locked && <span className="settings-row-locked">{locked}</span>}
      </div>
      <div className="settings-row-control">{children}</div>
    </div>
  );
}

/** An on/off switch styled as a settings toggle. */
export function Toggle({
  checked,
  onChange,
  label,
  disabled = false,
}: {
  checked: boolean;
  onChange: (value: boolean) => void;
  label: string;
  disabled?: boolean;
}) {
  return (
    <button
      type="button"
      role="switch"
      aria-checked={checked}
      aria-label={label}
      aria-disabled={disabled}
      disabled={disabled}
      className={`settings-toggle${checked ? ' on' : ''}${disabled ? ' disabled' : ''}`}
      onClick={() => !disabled && onChange(!checked)}
    >
      <span className="settings-toggle-thumb" />
    </button>
  );
}
