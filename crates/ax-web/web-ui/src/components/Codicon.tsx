/** VS Code codicon wrapper — requires @vscode/codicons/dist/codicon.css */

export default function Codicon({
  name,
  className = '',
  title,
}: {
  name: string;
  className?: string;
  title?: string;
}) {
  return (
    <i
      className={`codicon codicon-${name}${className ? ` ${className}` : ''}`}
      aria-hidden={title ? undefined : true}
      title={title}
    />
  );
}
