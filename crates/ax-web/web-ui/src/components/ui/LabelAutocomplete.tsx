import { useEffect, useId, useMemo, useRef, useState, type KeyboardEvent } from 'react';

export interface LabelAutocompleteProps {
  options: string[];
  selected: string[];
  onSelectedChange: (tags: string[]) => void;
  /** Free-text query (live) — filters by id/name/description in addition to labels. */
  query: string;
  onQueryChange: (query: string) => void;
  placeholder?: string;
  ariaLabel?: string;
}

function norm(s: string) {
  return s.trim().toLowerCase();
}

export function LabelAutocomplete({
  options,
  selected,
  onSelectedChange,
  query,
  onQueryChange,
  placeholder = 'Search or add label…',
  ariaLabel = 'Filter by labels',
}: LabelAutocompleteProps) {
  const listId = useId();
  const rootRef = useRef<HTMLDivElement>(null);
  const inputRef = useRef<HTMLInputElement>(null);
  const [open, setOpen] = useState(false);
  const [active, setActive] = useState(0);

  const selectedSet = useMemo(() => new Set(selected.map(norm)), [selected]);

  const suggestions = useMemo(() => {
    const needle = norm(query);
    return options
      .filter((opt) => !selectedSet.has(norm(opt)))
      .filter((opt) => !needle || norm(opt).includes(needle))
      .slice(0, 12);
  }, [options, selectedSet, query]);

  useEffect(() => {
    setActive(0);
  }, [query, open, suggestions.length]);

  useEffect(() => {
    function onDocPointer(e: MouseEvent) {
      if (!rootRef.current?.contains(e.target as Node)) setOpen(false);
    }
    document.addEventListener('mousedown', onDocPointer);
    return () => document.removeEventListener('mousedown', onDocPointer);
  }, []);

  function addTag(tag: string) {
    const t = tag.trim();
    if (!t || selectedSet.has(norm(t))) return;
    onSelectedChange([...selected, t]);
    onQueryChange('');
    setOpen(false);
    inputRef.current?.focus();
  }

  function removeTag(tag: string) {
    onSelectedChange(selected.filter((t) => norm(t) !== norm(tag)));
  }

  function onKeyDown(e: KeyboardEvent<HTMLInputElement>) {
    if (e.key === 'ArrowDown') {
      if (!open && suggestions.length) setOpen(true);
      e.preventDefault();
      setActive((i) => Math.min(i + 1, Math.max(suggestions.length - 1, 0)));
      return;
    }
    if (e.key === 'ArrowUp') {
      e.preventDefault();
      setActive((i) => Math.max(i - 1, 0));
      return;
    }
    if (e.key === 'Escape') {
      setOpen(false);
      return;
    }
    if (e.key === 'Backspace' && !query && selected.length > 0) {
      removeTag(selected[selected.length - 1]!);
      return;
    }
    if (e.key === 'Enter' || e.key === 'Tab') {
      if (open && suggestions.length > 0) {
        const pick = suggestions[active] ?? suggestions[0];
        if (pick) {
          e.preventDefault();
          addTag(pick);
        }
      }
    }
    if (e.key === ',' || e.key === ' ') {
      const exact = options.find((o) => norm(o) === norm(query));
      if (exact) {
        e.preventDefault();
        addTag(exact);
      }
    }
  }

  return (
    <div className="label-ac" ref={rootRef}>
      <div
        className={`label-ac-box${open ? ' label-ac-box--open' : ''}`}
        onClick={() => inputRef.current?.focus()}
      >
        {selected.map((tag) => (
          <button
            key={tag}
            type="button"
            className="label-ac-chip"
            onClick={(e) => {
              e.stopPropagation();
              removeTag(tag);
            }}
            aria-label={`Remove label ${tag}`}
          >
            <span>{tag}</span>
            <span className="label-ac-chip-x" aria-hidden="true">×</span>
          </button>
        ))}
        <input
          ref={inputRef}
          className="label-ac-input"
          type="search"
          value={query}
          placeholder={selected.length === 0 ? placeholder : 'Add label or search…'}
          aria-label={ariaLabel}
          aria-autocomplete="list"
          aria-expanded={open}
          aria-controls={listId}
          role="combobox"
          autoComplete="off"
          onChange={(e) => {
            onQueryChange(e.target.value);
            setOpen(true);
          }}
          onFocus={() => setOpen(true)}
          onKeyDown={onKeyDown}
        />
      </div>
      {open && suggestions.length > 0 && (
        <ul id={listId} className="label-ac-menu" role="listbox">
          {suggestions.map((opt, i) => (
            <li key={opt} role="option" aria-selected={i === active}>
              <button
                type="button"
                className={`label-ac-option${i === active ? ' active' : ''}`}
                onMouseEnter={() => setActive(i)}
                onMouseDown={(e) => {
                  e.preventDefault();
                  addTag(opt);
                }}
              >
                <span className="label-ac-option-label">{opt}</span>
              </button>
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}
