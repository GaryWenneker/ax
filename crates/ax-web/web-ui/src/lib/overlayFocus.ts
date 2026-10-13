/** Native inert and focus containment shared by sheets, drawers and inspectors. */
export function containOverlayFocus(panel: HTMLElement, onClose: () => void) {
  const previous = document.activeElement instanceof HTMLElement ? document.activeElement : null;
  const overflow = document.body.style.overflow;
  const inertSiblings: HTMLElement[] = [];
  let branch: HTMLElement = panel;
  while (branch.parentElement) {
    for (const sibling of Array.from(branch.parentElement.children)) {
      if (sibling instanceof HTMLElement && sibling !== branch && !sibling.inert && !sibling.matches('.sidebar-overlay, .mcp-q-backdrop')) {
        sibling.inert = true;
        inertSiblings.push(sibling);
      }
    }
    branch = branch.parentElement;
    if (branch === document.body) break;
  }
  const focusable = () => Array.from(panel.querySelectorAll<HTMLElement>('button, a[href], input, select, textarea, summary, [tabindex]'))
    .filter(el => el.tabIndex >= 0 && !el.closest('[inert]') && !el.matches(':disabled') && el.getClientRects().length > 0);
  const initial = panel.querySelector<HTMLElement>('.ax-modal-close, .detail-close[aria-label="Close"], .policy-rule-detail-back, .drawer-close, .project-browser-close, .files-preview-close') ?? focusable().find(el => !el.matches('input,textarea,select'));
  initial?.focus({preventScroll:true});
  document.body.style.overflow = 'hidden';
  function onKey(event: KeyboardEvent) {
    // A nested overlay owns its own Escape and focus cycle.
    if (panel.inert || panel.closest('[inert]')) return;
    if (event.key === 'Escape') {
      event.preventDefault();
      event.stopImmediatePropagation();
      onClose();
    } else if (event.key === 'Tab') {
      const items = focusable();
      const first = items[0], last = items[items.length - 1];
      if (!first) {event.preventDefault(); return;}
      if (event.shiftKey && (document.activeElement === first || !panel.contains(document.activeElement))) {
        event.preventDefault(); last.focus();
      } else if (!event.shiftKey && (document.activeElement === last || !panel.contains(document.activeElement))) {
        event.preventDefault(); first.focus();
      }
    }
  }
  document.addEventListener('keydown', onKey, true);
  return () => {
    document.removeEventListener('keydown', onKey, true);
    inertSiblings.forEach(el => {el.inert = false;});
    document.body.style.overflow = overflow;
    if (previous?.isConnected && !previous.closest('[inert]')) previous.focus({preventScroll:true});
  };
}
