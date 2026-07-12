import {
  materialIconUrl,
  resolveFileIconId,
  resolveFolderIconId,
} from '../lib/materialIconManifest';

export default function FileTypeIcon({
  fileName,
  language,
  kind = 'file',
  open = false,
  isRoot = false,
  className = '',
  size = 16,
  title,
}: {
  fileName: string;
  language?: string;
  kind?: 'file' | 'folder' | 'root';
  open?: boolean;
  isRoot?: boolean;
  className?: string;
  size?: number;
  title?: string;
}) {
  const iconId =
    kind === 'file'
      ? resolveFileIconId(fileName, language)
      : resolveFolderIconId(fileName, open, isRoot || kind === 'root');

  return (
    <img
      src={materialIconUrl(iconId)}
      alt=""
      aria-hidden={title ? undefined : true}
      title={title}
      className={`file-type-icon${className ? ` ${className}` : ''}`}
      width={size}
      height={size}
      draggable={false}
    />
  );
}
