import { spawn } from 'node:child_process';
import { randomBytes } from 'node:crypto';
import { existsSync } from 'node:fs';
import { join } from 'node:path';
import * as vscode from 'vscode';
import {
  PanelHost,
  STATUS_BAR_TEXT,
  STATUS_BAR_TOOLTIP,
  launchArgs,
  preparePopout,
  rowAction,
  switchProject,
  webviewHtml,
} from './core.ts';

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

/** Starts `ax web` for the workspace project and waits until it answers. */
async function ensureServer(port: number, binary: string, projectPath: string): Promise<void> {
  if (await isUp(port)) return;
  const args = launchArgs(port, projectPath);
  let spawnError: string | undefined;
  const child = spawn(binary, args, { cwd: projectPath, detached: true, stdio: 'ignore', shell: process.platform === 'win32' });
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
    const paint = (): void => {
      const { port: current } = settings();
      panel.webview.html = webviewHtml(current, randomBytes(16).toString('hex'));
    };
    paint();
    context.subscriptions.push(panel);
    return { reveal: () => panel.reveal(), reload: paint, onDispose: (fn) => panel.onDidDispose(fn) };
  });

  const open = async (): Promise<void> => {
    const { port, binary } = settings();
    const folders = vscode.workspace.workspaceFolders?.map((folder) => folder.uri.fsPath) ?? [];
    const editor = vscode.window.activeTextEditor;
    const activeFile = editor?.document.uri.scheme === 'file' ? editor.document.uri.fsPath : undefined;
    try {
      const prepared = await preparePopout({
        port,
        folders,
        activeFile,
        hasAxDb: (root) => existsSync(join(root, '.ax', 'ax.db')),
        ensureServer: (currentPort, projectPath) => ensureServer(currentPort, binary, projectPath),
        fetchImpl: fetch,
      });
      if (!prepared.ok) {
        const text = prepared.kind === 'start' ? `ax web did not start: ${prepared.message}` : prepared.message;
        const choice = await vscode.window.showErrorMessage(text, 'Retry');
        if (choice === 'Retry') await open();
        return;
      }
      const picked = await vscode.window.showQuickPick(
        prepared.rows.map((row) => ({ label: row.label, detail: row.detail, row })),
        { title: 'ax Command Center', placeHolder: 'Open Command Center or switch project' },
      );
      if (!picked) return;
      const action = rowAction(picked.row);
      if (action.switchPath) {
        const switched = await switchProject(port, action.switchPath, fetch);
        if (!switched.ok) {
          await vscode.window.showErrorMessage(switched.message);
          return;
        }
      }
      host.open();
    } catch (e) {
      const reason = e instanceof Error ? e.message : String(e);
      const choice = await vscode.window.showErrorMessage(`ax web did not start: ${reason}`, 'Retry');
      if (choice === 'Retry') await open();
    }
  };

  const status = vscode.window.createStatusBarItem(vscode.StatusBarAlignment.Right, 100);
  status.text = STATUS_BAR_TEXT;
  status.tooltip = STATUS_BAR_TOOLTIP;
  status.command = 'ax.openCommandCenter';
  status.show();

  context.subscriptions.push(vscode.commands.registerCommand('ax.openCommandCenter', open), status);
}

export function deactivate(): void {}
