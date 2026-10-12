import { useEffect, useState } from 'react';
import { ACCENT_COLORS, THEMES, THEME_CHANGED, accentForTheme, applyTheme, loadAccentId, saveThemeId, selectAccent, themeById, type ThemePreset } from '../lib/themes';

export default function ThemeChooser({ activeId, onSelect }: { activeId: string; onSelect: (id: string) => void }) {
  const [hoveredId, setHoveredId] = useState<string | null>(null);
  const [accentId, setAccentId] = useState(loadAccentId);
  useEffect(() => {
    const update = () => setAccentId(loadAccentId());
    window.addEventListener(THEME_CHANGED,update);
    return () => window.removeEventListener(THEME_CHANGED,update);
  },[]);

  function select(theme: ThemePreset) {
    saveThemeId(theme.id);
    applyTheme(theme);
    onSelect(theme.id);
  }

  return (
    <>
    <div className="theme-chooser">
      {THEMES.map((base) => {
        const theme = {...base,accent:accentForTheme(base,accentId)};
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
    <div className="accent-chooser">
      <span className="settings-row-title">Accent color</span>
      <p className="settings-row-desc">Lime matches the Ax website. Each color adapts to Light and Dark for readable controls. Header and footer backgrounds stay unchanged; error and warning colors keep their meaning.</p>
      <div className="accent-options" role="group" aria-label="Accent color">
        {ACCENT_COLORS.map(color => <button key={color.id} type="button" className="btn accent-option" aria-pressed={accentId === color.id} onClick={() => {selectAccent(color.id);setAccentId(color.id);}}>
          <span aria-hidden="true" className="accent-dot" style={{background:accentForTheme(themeById(activeId),color.id)}} />{color.label}{color.id === 'lime' ? ' (default)' : ''}
        </button>)}
      </div>
    </div>
    </>
  );
}
