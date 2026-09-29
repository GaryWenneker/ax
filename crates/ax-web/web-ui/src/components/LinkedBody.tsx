import { useEffect, useState, type MouseEvent } from 'react';
import MarkdownPreview from './MarkdownPreview';
import Codicon from './Codicon';
import {
  fetchItemLinks,
  linkHref,
  missingTitle,
  MISSING_HREF,
  splitWikiLinks,
  wikiLinksToMarkdown,
  type Backlink,
  type ItemLinks,
  type LinkKind,
} from '../wikilinks';

const KIND_ICON: Record<LinkKind, string> = { rule: 'law', skill: 'tools', memory: 'note' };

const REFETCH_DELAY_MS = 400;

/** Links of one item, refetched shortly after its body changes (links come from the saved
 *  body, so a link typed but not saved stays plain text). `null` until loaded or on error. */
export function useItemLinks(kind: LinkKind, id: string | null | undefined, origin: string | undefined, body: string): ItemLinks | null {
  const [links, setLinks] = useState<ItemLinks | null>(null);
  const [loadedFor, setLoadedFor] = useState('');
  const key = id ? `${kind}:${origin ?? ''}:${id}` : '';
  useEffect(() => {
    if (!key || !id) {
      setLinks(null);
      return;
    }
    let live = true;
    const load = () =>
      fetchItemLinks(kind, id, origin)
        .then((l) => { if (live) { setLinks(l); setLoadedFor(key); } })
        .catch(() => { if (live) setLinks(null); });
    const timer = setTimeout(load, loadedFor === key ? REFETCH_DELAY_MS : 0);
    return () => { live = false; clearTimeout(timer); };
    // eslint-disable-next-line react-hooks/exhaustive-deps -- loadedFor only picks the delay
  }, [key, body]);
  return links;
}

/** Opens Command Center links in place; the missing-link marker goes nowhere. */
export function interceptLinkClick(e: MouseEvent<HTMLElement>) {
  const a = (e.target as HTMLElement).closest('a');
  const href = a?.getAttribute('href');
  if (!href) return;
  if (href === MISSING_HREF) {
    e.preventDefault();
    return;
  }
  const inApp = href.startsWith('/policy/') || href.startsWith('/memory?');
  if (!inApp || e.metaKey || e.ctrlKey || e.shiftKey || e.button !== 0) return;
  e.preventDefault();
  window.history.pushState(null, '', href);
  window.dispatchEvent(new PopStateEvent('popstate'));
}

export function Backlinks({ backlinks }: { backlinks: Backlink[] | undefined }) {
  if (!backlinks || backlinks.length === 0) return null;
  return (
    <div className="wikilink-backlinks" onClick={interceptLinkClick}>
      <div className="detail-section-title">Linked from ({backlinks.length})</div>
      <div className="edge-list">
        {backlinks.map((b) => (
          <a key={`${b.kind}:${b.origin}:${b.id}`} className="edge-item wikilink-backlink" href={linkHref(b)}>
            <Codicon name={KIND_ICON[b.kind]} className="edge-item-icon" />
            <span className="edge-name">{b.title}</span>
            <span className="wikilink-backlink-kind">{b.origin === 'global' ? `global ${b.kind}` : b.kind}</span>
          </a>
        ))}
      </div>
    </div>
  );
}

/** A rule or skill body with its `[[links]]` clickable, followed by its backlinks. */
export function LinkedMarkdown({ links, value, className }: { links: ItemLinks | null; value: string; className?: string }) {
  const source = links ? wikiLinksToMarkdown(value, links.outgoing) : value;
  return (
    <>
      <MarkdownPreview value={source} className={className} onClick={interceptLinkClick} />
      <Backlinks backlinks={links?.backlinks} />
    </>
  );
}

/** Plain text (memory prose) with its `[[links]]` as links. */
export function LinkedProse({ text, links }: { text: string; links: ItemLinks | null }) {
  const byText = new Map((links?.outgoing ?? []).map((l) => [l.text, l]));
  return (
    <>
      {splitWikiLinks(text).map((seg, i) => {
        if ('text' in seg) return <span key={i}>{seg.text}</span>;
        const link = byText.get(seg.link);
        if (!link) return <span key={i}>{seg.link}</span>;
        const label = link.label ?? link.target;
        if (!link.resolved) {
          return (
            <span key={i} className="wikilink-missing" title={missingTitle(link.target)}>
              {label}
            </span>
          );
        }
        return (
          <a key={i} className="wikilink" href={linkHref(link.resolved)} onClick={interceptLinkClick}>
            {label}
          </a>
        );
      })}
    </>
  );
}
