import { useState } from 'react';
import { THEMES, applyTheme, saveThemeId, type ThemePreset } from '../lib/themes';

export default function ThemeChooser({ activeId, onSelect }: { activeId: string; onSelect: (id: string) => void }) {
  const [hoveredId, setHoveredId] = useState<string | null>(null);

  function select(theme: ThemePreset) {
    applyTheme(theme);
    saveThemeId(theme.id);
    onSelect(theme.id);
  }

  return (
    <div className="theme-chooser">
      {THEMES.map((theme) => {
        const active = theme.id === activeId;
        const hovered = theme.id === hoveredId;
        return (
          <button
            key={theme.id}
            type="button"
            className={`theme-swatch${active ? ' theme-swatch--active' : ''}`}
            title={theme.label}
            aria-label={`${theme.label}${active ? ' (current)' : ''}`}
            aria-pressed={active}
            onClick={() => select(theme)}
            onMouseEnter={() => setHoveredId(theme.id)}
            onMouseLeave={() => setHoveredId(null)}
          >
            <div className="theme-swatch-preview" style={{ background: theme.bg }}>
              <div className="theme-swatch-sidebar" style={{ background: theme.bgSide }} />
              <div className="theme-swatch-content">
                <div className="theme-swatch-bar" style={{ background: theme.accent }} />
                <div className="theme-swatch-line" style={{ background: theme.borderHi }} />
                <div className="theme-swatch-line theme-swatch-line--short" style={{ background: theme.borderHi }} />
              </div>
            </div>
            <span className="theme-swatch-dot" style={{ background: theme.accent }} />
            <span className="theme-swatch-label">{theme.label}</span>
            {(active || hovered) && (
              <span className="theme-swatch-check" style={{ color: active ? theme.accent : theme.textDim }}>
                {active ? '\u2713' : ''}
              </span>
            )}
          </button>
        );
      })}
    </div>
  );
}
