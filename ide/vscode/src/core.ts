function checkPort(port: number): number {
  if (!Number.isInteger(port) || port < 1 || port > 65535) {
    throw new Error(`ax.webPort must be a port number between 1 and 65535, got ${port}`);
  }
  return port;
}

export function commandCenterUrl(port: number): string {
  return `http://127.0.0.1:${checkPort(port)}/?embed=1`;
}

/** The webview only frames the local Command Center; it runs no script of its own. */
export function webviewHtml(port: number, nonce: string): string {
  const origin = `http://127.0.0.1:${checkPort(port)}`;
  const csp = `default-src 'none'; frame-src ${origin}; style-src 'nonce-${nonce}';`;
  return `<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="UTF-8">
<meta http-equiv="Content-Security-Policy" content="${csp}">
<style nonce="${nonce}">html,body,iframe{margin:0;padding:0;border:0;width:100%;height:100%;overflow:hidden}</style>
</head>
<body><iframe src="${commandCenterUrl(port)}" title="ax Command Center"></iframe></body>
</html>`;
}

export interface PanelLike {
  reveal(): void;
  onDispose(fn: () => void): void;
}

/** Keeps a single Command Center panel: a second open reveals it instead of opening another. */
export class PanelHost {
  private readonly create: () => PanelLike;
  private panel: PanelLike | undefined;

  constructor(create: () => PanelLike) {
    this.create = create;
  }

  open(): void {
    if (this.panel) {
      this.panel.reveal();
      return;
    }
    const panel = this.create();
    panel.onDispose(() => {
      if (this.panel === panel) this.panel = undefined;
    });
    this.panel = panel;
  }
}
