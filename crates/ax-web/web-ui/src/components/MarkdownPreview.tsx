import type { MouseEvent } from 'react';
import MDEditor from '@uiw/react-md-editor';

interface Props {
  value: string;
  className?: string;
  openLinksInNewTab?: boolean;
  /** Replacement URL for an image `src`; `null` keeps the image as written. */
  imageSrc?: (src: string) => string | null;
  onImageClick?: (src: string, alt: string) => void;
  onClick?: (e: MouseEvent<HTMLDivElement>) => void;
}

export default function MarkdownPreview({
  value,
  className = '',
  openLinksInNewTab = false,
  imageSrc,
  onImageClick,
  onClick,
}: Props) {
  const handleClick = onImageClick || onClick
    ? (e: MouseEvent<HTMLDivElement>) => {
        onClick?.(e);
        if (!onImageClick || e.defaultPrevented || !(e.target instanceof HTMLImageElement)) return;
        e.preventDefault();
        onImageClick(e.target.currentSrc || e.target.src, e.target.alt);
      }
    : undefined;
  return (
    <div
      className={`md-preview-wrap${onImageClick ? ' md-preview-wrap--zoomable' : ''}${className ? ` ${className}` : ''}`}
      data-color-mode="dark"
      onClick={handleClick}
    >
      <MDEditor.Markdown
        source={value || '_No content._'}
        rehypeRewrite={openLinksInNewTab || imageSrc ? (node) => {
          if (node.type !== 'element' || !node.properties) return;
          if (openLinksInNewTab && node.tagName === 'a') {
            node.properties.target = '_blank';
            node.properties.rel = 'noopener noreferrer';
          }
          if (imageSrc && node.tagName === 'img' && typeof node.properties.src === 'string') {
            const next = imageSrc(node.properties.src);
            if (next !== null) node.properties.src = next;
          }
        } : undefined}
      />
    </div>
  );
}
