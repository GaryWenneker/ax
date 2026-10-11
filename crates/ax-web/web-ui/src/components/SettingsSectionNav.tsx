import { useEffect, useMemo, useState } from 'react';

import {
  SETTINGS_SECTION_GROUPS,
  SETTINGS_SECTIONS,
  isSettingsSectionId,
  settingsSectionLabel,
} from '../lib/settingsSections';

function currentHashSection(): string {
  const id = window.location.hash.replace(/^#/, '');
  return isSettingsSectionId(id) ? id : SETTINGS_SECTIONS[0]?.id ?? '';
}

function reducedMotion(): boolean {
  return window.matchMedia?.('(prefers-reduced-motion: reduce)').matches ?? false;
}

function scrollToSection(id: string, pushHistory = true) {
  const target = document.getElementById(id);
  if (!target) return;
  if (pushHistory) {
    history.pushState(null, '', '#' + id);
  } else {
    history.replaceState(null, '', '#' + id);
  }
  target.scrollIntoView({
    behavior: reducedMotion() ? 'auto' : 'smooth',
    block: 'start',
  });
}

export default function SettingsSectionNav() {
  const [activeId, setActiveId] = useState(() => currentHashSection());
  const ids = useMemo(() => SETTINGS_SECTIONS.map((section) => section.id), []);

  useEffect(() => {
    const initial = currentHashSection();
    if (window.location.hash && isSettingsSectionId(initial)) {
      requestAnimationFrame(() => scrollToSection(initial, false));
    }

    const elements = ids
      .map((id) => document.getElementById(id))
      .filter((element): element is HTMLElement => Boolean(element));
    if (!elements.length || !('IntersectionObserver' in window)) return;

    const observer = new IntersectionObserver(
      (entries) => {
        const visible = entries
          .filter((entry) => entry.isIntersecting)
          .sort((a, b) => a.boundingClientRect.top - b.boundingClientRect.top);
        const next = visible[0]?.target.id;
        if (!next || !isSettingsSectionId(next)) return;
        setActiveId(next);
        if (window.location.hash !== '#' + next) {
          history.replaceState(null, '', '#' + next);
        }
      },
      { rootMargin: '-12% 0px -72% 0px', threshold: [0, 0.01, 0.25] },
    );

    elements.forEach((element) => observer.observe(element));
    return () => observer.disconnect();
  }, [ids]);

  useEffect(() => {
    const onPopState = () => {
      const id = currentHashSection();
      setActiveId(id);
      requestAnimationFrame(() => scrollToSection(id, false));
    };
    window.addEventListener('popstate', onPopState);
    return () => window.removeEventListener('popstate', onPopState);
  }, []);

  function activate(id: string) {
    setActiveId(id);
    scrollToSection(id);
  }

  return (
    <>
      <nav className="settings-section-nav" aria-label="Settings sections">
        {SETTINGS_SECTION_GROUPS.map((group) => (
          <div className="settings-section-nav-group" key={group.id}>
            <div className="settings-section-nav-label">{group.label}</div>
            {group.sections.map((section) => (
              <button
                type="button"
                key={section.id}
                className={'settings-section-nav-item' + (activeId === section.id ? ' active' : '')}
                aria-current={activeId === section.id ? 'location' : undefined}
                onClick={() => activate(section.id)}
              >
                {section.label}
              </button>
            ))}
          </div>
        ))}
      </nav>

      <div className="settings-section-mobile">
        <label htmlFor="settings-section-select">Settings section</label>
        <select
          id="settings-section-select"
          className="settings-select"
          value={activeId}
          onChange={(event) => activate(event.target.value)}
        >
          {SETTINGS_SECTION_GROUPS.map((group) => (
            <optgroup key={group.id} label={group.label}>
              {group.sections.map((section) => (
                <option key={section.id} value={section.id}>
                  {settingsSectionLabel(section.id)}
                </option>
              ))}
            </optgroup>
          ))}
        </select>
      </div>
    </>
  );
}
