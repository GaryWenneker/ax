import { useEffect, useRef, useState } from 'react';
import { createPortal } from 'react-dom';
import { usePinnedPopover } from '../lib/placePopover';

type Member = { id: string; display_name: string; email?: string | null; role?: unknown; avatar?: string | null };

const photoKey = (id: string) => `ax-profile-photo:${id}`;

function initials(name: string): string {
  const letters = name.split(/\s+/).filter(Boolean).slice(0, 2).map((part) => part[0]?.toUpperCase() ?? '');
  return letters.join('') || '?';
}

function roleLabel(role: unknown): string {
  return typeof role === 'string' ? role : '';
}

/** Account menu shown only after a required Command Center sign-in. */
export default function ProfileMenu({ onSettings, onSignedIn }: { onSettings: () => void; onSignedIn?: (signedIn: boolean) => void }) {
  const [member, setMember] = useState<Member | null>(null);
  const [open, setOpen] = useState(false);
  const [photo, setPhoto] = useState<string | null>(null);
  const root = useRef<HTMLDivElement>(null);
  const panel = useRef<HTMLDivElement>(null);
  const button = useRef<HTMLButtonElement>(null);
  const file = useRef<HTMLInputElement>(null);
  usePinnedPopover(open, button, panel, { prefer: 'below', align: 'end' });

  useEffect(() => {
    let cancelled = false;
    fetch('/api/web-auth/session', { credentials: 'same-origin', cache: 'no-store' })
      .then(async (response) => (response.ok ? response.json() as Promise<{ authenticated?: boolean; member?: Member }> : null))
      .then((data) => {
        if (cancelled) return;
        const next = data?.authenticated && data.member ? data.member : null;
        setMember(next);
        onSignedIn?.(next !== null);
      })
      .catch(() => { if (!cancelled) { setMember(null); onSignedIn?.(false); } });
    return () => { cancelled = true; };
  }, []);

  useEffect(() => {
    if (!member) {
      setPhoto(null);
      return;
    }
    const saved = member.avatar || null;
    const local = localStorage.getItem(photoKey(member.id));
    if (saved) {
      setPhoto(saved);
      if (local) localStorage.removeItem(photoKey(member.id));
      return;
    }
    setPhoto(local);
    if (!local) return;
    void fetch('/api/web-auth/avatar', {
      method: 'POST',
      credentials: 'same-origin',
      headers: { 'content-type': 'application/json' },
      body: JSON.stringify({ image: local }),
    }).then((response) => {
      if (response.ok) localStorage.removeItem(photoKey(member.id));
    }).catch(() => undefined);
  }, [member]);

  useEffect(() => {
    if (!open) return;
    const close = (event: MouseEvent) => {
      const target = event.target as Node;
      if (root.current?.contains(target) || panel.current?.contains(target)) return;
      setOpen(false);
    };
    const onKey = (event: KeyboardEvent) => { if (event.key === 'Escape') setOpen(false); };
    window.addEventListener('mousedown', close);
    window.addEventListener('keydown', onKey);
    return () => {
      window.removeEventListener('mousedown', close);
      window.removeEventListener('keydown', onKey);
    };
  }, [open]);

  if (!member) return null;

  async function storePhoto(next: string | null) {
    if (!member) return;
    const response = await fetch('/api/web-auth/avatar', {
      method: 'POST',
      credentials: 'same-origin',
      headers: { 'content-type': 'application/json' },
      body: JSON.stringify({ image: next }),
    });
    if (!response.ok) return;
    localStorage.removeItem(photoKey(member.id));
    setPhoto(next);
  }

  function readPhoto(list: FileList | null) {
    const picked = list?.[0];
    if (!picked?.type.startsWith('image/')) return;
    const reader = new FileReader();
    reader.onload = () => {
      const image = new Image();
      image.onload = () => {
        const canvas = document.createElement('canvas');
        canvas.width = 128;
        canvas.height = 128;
        const ctx = canvas.getContext('2d');
        if (!ctx) return;
        const scale = Math.max(128 / image.width, 128 / image.height);
        const width = image.width * scale;
        const height = image.height * scale;
        ctx.drawImage(image, (128 - width) / 2, (128 - height) / 2, width, height);
        void storePhoto(canvas.toDataURL('image/jpeg', 0.85));
      };
      image.src = String(reader.result);
    };
    reader.readAsDataURL(picked);
  }

  async function logout() {
    await fetch('/api/web-auth/logout', { method: 'POST', credentials: 'same-origin' });
    window.dispatchEvent(new Event('ax-web-auth-changed'));
    window.location.reload();
  }

  const label = roleLabel(member.role);
  return (
    <div className="profile-menu" ref={root}>
      <button ref={button} type="button" className="profile-menu-button" aria-haspopup="menu" aria-expanded={open}
        aria-label={`Account menu for ${member.display_name}`} onClick={() => setOpen((value) => !value)}>
        {photo ? <img src={photo} alt="" /> : <span>{initials(member.display_name)}</span>}
      </button>
      {open && createPortal(
        <div ref={panel} className="profile-menu-panel" role="menu">
          <div className="profile-menu-id">
            <strong>{member.display_name}</strong>
            {member.email ? <span>{member.email}</span> : null}
            {label ? <span className="profile-menu-role">{label}</span> : null}
          </div>
          <button type="button" role="menuitem" onClick={() => { setOpen(false); onSettings(); }}>Settings</button>
          <button type="button" role="menuitem" onClick={() => file.current?.click()}>Change photo</button>
          {photo && <button type="button" role="menuitem" onClick={() => void storePhoto(null)}>Remove photo</button>}
          <button type="button" role="menuitem" className="profile-menu-logout" onClick={() => void logout()}>Log out</button>
          <p className="profile-menu-note">Saved with your account on this installation.</p>
          <input ref={file} type="file" accept="image/*" hidden onChange={(event) => { readPhoto(event.target.files); event.target.value = ''; }} />
        </div>,
        document.body,
      )}
    </div>
  );
}
