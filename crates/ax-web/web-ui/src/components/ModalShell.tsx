import { useEffect, useRef, type ReactNode } from 'react';
import { createPortal } from 'react-dom';

import Codicon from './Codicon';
import {containOverlayFocus} from '../lib/overlayFocus';

export interface ModalShellProps {
  title: string;
  subtitle?: string;
  onClose: () => void;
  children: ReactNode;
  footer?: ReactNode;
  /** Default 520px; lg = 720px; xl = 1120px; full = near-viewport restore inspect */
  size?: 'md' | 'lg' | 'xl' | 'full';
  ariaLabel?: string;
}

export default function ModalShell({
  title,
  subtitle,
  onClose,
  children,
  footer,
  size = 'md',
  ariaLabel,
}: ModalShellProps) {
  const panel = useRef<HTMLDivElement>(null);
  const close = useRef(onClose);
  useEffect(() => {close.current = onClose;}, [onClose]);
  useEffect(() => {
    if (panel.current) return containOverlayFocus(panel.current, () => close.current());
  }, []);

  return createPortal(
    <div className="ax-modal-overlay" role="presentation" onMouseDown={onClose}>
      <div
        ref={panel}
        className={`ax-modal ax-modal--${size}`}
        role="dialog"
        aria-modal="true"
        aria-label={ariaLabel ?? title}
        onMouseDown={(e) => e.stopPropagation()}
      >
        <header className="ax-modal-header">
          <div>
            <h2 className="ax-modal-title">{title}</h2>
            {subtitle && <p className="ax-modal-subtitle">{subtitle}</p>}
          </div>
          <button type="button" className="ax-modal-close btn btn-subtle" onClick={onClose} aria-label="Close">
            <Codicon name="close" />
          </button>
        </header>
        <div className="ax-modal-body">{children}</div>
        {footer && <footer className="ax-modal-footer">{footer}</footer>}
      </div>
    </div>,
    document.body,
  );
}
