import * as vscode from 'vscode';

/**
 * Custom text content provider for `.shard/logs/*.log` files.
 * Strips ANSI escape sequences so the raw log is readable without
 * terminal artifacts in the editor.
 */
export class LogFileProvider implements vscode.TextDocumentContentProvider {
  static scheme = 'shard-log';
  private _onDidChange = new vscode.EventEmitter<vscode.Uri>();
  readonly onDidChange = this._onDidChange.event;

  provideTextDocumentContent(uri: vscode.Uri): string {
    const filePath = uri.path;
    try {
      const raw = require('fs').readFileSync(filePath, 'utf8');
      return stripAnsi(raw);
    } catch {
      return `// Unable to read log: ${filePath}`;
    }
  }

  /** Register a virtual document URI for the real log path. */
  static asUri(logPath: string): vscode.Uri {
    return vscode.Uri.from({
      scheme: LogFileProvider.scheme,
      path: logPath,
    });
  }
}

function stripAnsi(text: string): string {
  // Remove ANSI escape sequences: \x1b[...m, \x1b[...H, \x1b[...J, etc.
  return text.replace(/\x1b\[[0-9;]*[a-zA-Z]/g, '')
    .replace(/\x1b\][^\x1b]*\x1b\\/g, '')
    .replace(/\x1b\][^\x07]*\x07/g, '');
}
