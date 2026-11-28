import type { KeyboardEvent } from 'react';

type KeyLike = Pick<KeyboardEvent, 'key' | 'target' | 'currentTarget' | 'preventDefault'>;

export interface ClickableRowProps {
  tabIndex: 0;
  onClick: () => void;
  onKeyDown: (e: KeyLike) => void;
  'aria-selected'?: boolean;
}

/**
 * Props that make a table row (or list row) that acts on click reachable with
 * Tab and activated with Enter or Space, so the action is not mouse-only.
 * Pass `selected` when the row shows a selection.
 */
export function clickableRowProps(onActivate: () => void, opts: { selected?: boolean } = {}): ClickableRowProps {
  const props: ClickableRowProps = {
    tabIndex: 0,
    onClick: onActivate,
    onKeyDown: (e) => {
      if (e.target !== e.currentTarget) return;
      if (e.key !== 'Enter' && e.key !== ' ') return;
      e.preventDefault();
      onActivate();
    },
  };
  if (opts.selected !== undefined) props['aria-selected'] = opts.selected;
  return props;
}
