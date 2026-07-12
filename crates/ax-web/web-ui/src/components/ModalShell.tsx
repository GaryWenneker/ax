import { useEffect, type ReactNode } from 'react';
import { createPortal } from 'react-dom';

import Codicon from './Codicon';

export interface ModalShellProps {
  title: string;
  subtitle?: string;
  onClose: () => void;
  children: ReactNode;
  footer?: ReactNode;
  /** Default 520px; lg = 720px */
  size?: 'md' | 'lg';
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
  useEffect(() => {
    function onKey(e: KeyboardEvent) {
      if (e.key === 'Escape') onClose();
    }
    document.addEventListener('keydown', onKey);
    const prev = document.body.style.overflow;
    document.body.style.overflow = 'hidden';
    return () => {
      document.removeEventListener('keydown', onKey);
      document.body.style.overflow = prev;
    };
  }, [onClose]);

  return createPortal(
    <div className="ax-modal-overlay" role="presentation" onMouseDown={onClose}>
      <div
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
