import { isGitShared } from './policyListUtils';

const TITLE = 'Shared via git (.agents)';

export function GitShareDot({
  scope,
  enabled,
}: {
  scope?: string;
  enabled?: boolean;
}) {
  if (!isGitShared(scope, enabled)) return null;
  return (
    <span className="git-share-dot" role="img" aria-label={TITLE} title={TITLE} />
  );
}
