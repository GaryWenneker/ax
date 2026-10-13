import {useState, type ReactNode} from 'react';
import useMobile from '../../hooks/useMobile';
import ModalShell from '../ModalShell';
import Codicon from '../Codicon';

export function ResponsiveSheet({title, onClose, children}: {title:string; onClose:()=>void; children:ReactNode}) {
  const mobile = useMobile();
  return mobile ? <ModalShell title={title} onClose={onClose} footer={<button type="button" className="btn primary" onClick={onClose}>Done</button>}>
    <div className="mobile-controls-body">{children}</div>
  </ModalShell> : <>{children}</>;
}

export default function MobileControls({children, className, title='Filters', triggerLabel=title, active=false, icon='filter'}: {
  children:ReactNode; className:string; title?:string; triggerLabel?:string; active?:boolean; icon?:string;
}) {
  const mobile = useMobile();
  const [open, setOpen] = useState(false);
  if (!mobile) return <div className={className}>{children}</div>;
  return <div className="mobile-controls">
    <button type="button" className={`btn mobile-controls-trigger${active ? ' mobile-controls-trigger--active' : ''}`}
      aria-label={triggerLabel} aria-haspopup="dialog" aria-expanded={open} onClick={() => setOpen(true)}>
      <Codicon name={icon} />{triggerLabel}{active && <span className="mobile-active-dot" aria-label="Active filters" />}
    </button>
    {open && <ResponsiveSheet title={title} onClose={() => setOpen(false)}><div className={className}>{children}</div></ResponsiveSheet>}
  </div>;
}
