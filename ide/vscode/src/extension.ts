import { spawn } from 'node:child_process';
import { randomBytes } from 'node:crypto';
import * as vscode from 'vscode';
import { PanelHost, webviewHtml } from './core.ts';

const START_TIMEOUT_MS = 15_000;

function settings(): { port: number; binary: string } {
  const cfg = vscode.workspace.getConfiguration('ax');
  return { port: cfg.get<number>('webPort', 7070), binary: cfg.get<string>('binary', 'ax') };
}

async function isUp(port: number): Promise<boolean> {
  try {
    const res = await fetch(`http://127.0.0.1:${port}/`, { signal: AbortSignal.timeout(1500) });
    return res.ok;
  } catch {
    return false;
  }
}

/** Starts `ax web` for the first workspace folder and waits until it answers. */
async function ensureServer(port: number, binary: string): Promise<void> {
  if (await isUp(port)) return;
  const folder = vscode.workspace.workspaceFolders?.[0]?.uri.fsPath;
  const args = ['web', '--port', String(port), ...(folder ? [folder] : [])];
  let spawnError: string | undefined;
  const child = spawn(binary, args, { cwd: folder, detached: true, stdio: 'ignore', shell: process.platform === 'win32' });
  child.on('error', (e) => (spawnError = e.message));
  child.unref();
  const deadline = Date.now() + START_TIMEOUT_MS;
  while (Date.now() < deadline) {
    if (spawnError) throw new Error(spawnError);
    if (await isUp(port)) return;
    await new Promise((r) => setTimeout(r, 500));
  }
  throw new Error(`no answer on port ${port} after ${START_TIMEOUT_MS / 1000} s`);
}

export function activate(context: vscode.ExtensionContext): void {
  const host = new PanelHost(() => {
    const { port } = settings();
    const panel = vscode.window.createWebviewPanel('axCommandCenter', 'ax Command Center', vscode.ViewColumn.Active, {
      // Sandbox flags are inherited by the iframe; the Command Center needs scripts. The CSP blocks ours.
      enableScripts: true,
      retainContextWhenHidden: true,
      portMapping: [{ webviewPort: port, extensionHostPort: port }],
    });
    panel.webview.html = webviewHtml(port, randomBytes(16).toString('hex'));
    context.subscriptions.push(panel);
    return { reveal: () => panel.reveal(), onDispose: (fn) => panel.onDidDispose(fn) };
  });

  const open = async (): Promise<void> => {
    const { port, binary } = settings();
    try {
      await ensureServer(port, binary);
      host.open();
    } catch (e) {
      const reason = e instanceof Error ? e.message : String(e);
      const choice = await vscode.window.showErrorMessage(`ax web did not start: ${reason}`, 'Retry');
      if (choice === 'Retry') await open();
    }
  };

  const status = vscode.window.createStatusBarItem(vscode.StatusBarAlignment.Right, 100);
  status.text = '$(graph) ax';
  status.tooltip = 'Open the ax Command Center';
  status.command = 'ax.openCommandCenter';
  status.show();

  context.subscriptions.push(vscode.commands.registerCommand('ax.openCommandCenter', open), status);
}

export function deactivate(): void {}
