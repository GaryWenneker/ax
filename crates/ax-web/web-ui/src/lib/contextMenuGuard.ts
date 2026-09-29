const EDITABLE = 'input, textarea, select, [contenteditable=""], [contenteditable="true"]';

export function keepsBrowserMenu(target: EventTarget | null): boolean {
  return target instanceof Element && target.closest(EDITABLE) !== null;
}

/** Blocks the browser context menu everywhere except editable fields; app menus still receive the event. */
export function installContextMenuGuard(doc: Document = document): () => void {
  const onContextMenu = (e: Event) => {
    if (!keepsBrowserMenu(e.target)) e.preventDefault();
  };
  doc.addEventListener('contextmenu', onContextMenu);
  return () => doc.removeEventListener('contextmenu', onContextMenu);
}
