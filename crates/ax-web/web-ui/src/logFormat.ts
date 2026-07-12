/** Normalize legacy unix-second timestamps in log lines to local date/time. */
export function formatLogLine(line: string): string {
  return line.replace(/^\[(\d{10,})\]/, (_, raw) => {
    const date = new Date(Number(raw) * 1000);
    if (Number.isNaN(date.getTime())) return line;
    return `[${date.toLocaleString()}]`;
  });
}

/** Human-readable label for what is currently running in the pipeline log. */
export function liveActivityLabel(
  lines: string[] | undefined,
  liveStep: string | null,
  liveSonarKey: string | null,
  sonarName?: string | null,
): string | null {
  if (liveSonarKey) {
    return sonarName ? `Sonar · ${sonarName}` : `Sonar · ${liveSonarKey}`;
  }
  if (liveStep) {
    const labels: Record<string, string> = {
      index: 'Indexing',
      diff: 'Diff',
      tia: 'Test impact',
      tests: 'Tests',
      sonar: 'Sonar',
      policy: 'Policy',
    };
    return labels[liveStep] ?? liveStep;
  }
  if (!lines?.length) return null;
  for (let i = lines.length - 1; i >= 0; i--) {
    const formatted = formatLogLine(lines[i]);
    const sonar = formatted.match(/▶ sonar:[\w.-]+ — (.+?) \(\d+\/\d+\)/);
    if (sonar) return `Sonar · ${sonar[1]}`;
    const step = formatted.match(/▶ (\w+)/);
    if (step) return step[1];
  }
  return null;
}
