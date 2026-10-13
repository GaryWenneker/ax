import {useEffect} from 'react';
import useMobile from '../hooks/useMobile';
import {containOverlayFocus} from '../lib/overlayFocus';

/** Existing shared inspectors stay mounted, preserving their data and callbacks. */
export default function MobileDetailLayer() {
  const mobile = useMobile();
  useEffect(() => {
    if (!mobile) return;
    const selector = '.detail-panel, .memory-blade, .policy-inline-workspace, .graph-settings, .files-preview-pane';
    let current: HTMLElement | null = null;
    let release: (() => void) | undefined;
    function update() {
      const candidates = Array.from(document.querySelectorAll<HTMLElement>(selector))
        .filter(el => !el.parentElement?.closest(selector) && el.getClientRects().length > 0);
      const panel = candidates[candidates.length - 1] ?? null;
      if (panel === current) return;
      release?.();release = undefined;current = panel;
      if (!panel) return;
      const close = panel.querySelector<HTMLButtonElement>('.detail-close[aria-label="Close"], .policy-rule-detail-back, .graph-settings-icon[aria-label="Close"], .files-preview-close');
      if (!close) return;
      const oldRole = panel.getAttribute('role'), oldModal = panel.getAttribute('aria-modal'), oldLabel = panel.getAttribute('aria-label');
      if (!oldLabel) panel.setAttribute('aria-label',panel.querySelector('.files-preview-path')?.textContent ?? 'Details');
      panel.dataset.mobileDetail = 'true';
      panel.setAttribute('role','dialog');panel.setAttribute('aria-modal','true');
      const restore = containOverlayFocus(panel, () => close.click());
      release = () => {
        restore();delete panel.dataset.mobileDetail;
        if (oldRole === null) panel.removeAttribute('role');else panel.setAttribute('role',oldRole);
        if (oldModal === null) panel.removeAttribute('aria-modal');else panel.setAttribute('aria-modal',oldModal);
        if (oldLabel === null) panel.removeAttribute('aria-label');else panel.setAttribute('aria-label',oldLabel);
      };
    }
    const observer = new MutationObserver(update);
    observer.observe(document.body,{childList:true,subtree:true});update();
    return () => {observer.disconnect();release?.();};
  }, [mobile]);
  return null;
}
