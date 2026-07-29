import * as vscode from 'vscode';
import { MetricsReader, HistoryRow } from './metrics';
import { LogFileProvider } from './logViewer';

/**
 * Explorer-panel tree view of recent Shard runs. Each item shows the argv
 * head, savings percentage, and tokens saved. Clicking opens the raw log
 * cache file (ANSI-stripped) via the custom log viewer.
 */
export class RecentRunsProvider implements vscode.TreeDataProvider<RunItem> {
  private emitter = new vscode.EventEmitter<RunItem | undefined>();
  readonly onDidChangeTreeData = this.emitter.event;

  private openEmitter = new vscode.EventEmitter<string>();
  readonly onDidOpenLog = this.openEmitter.event;

  constructor(private readonly reader: MetricsReader) {}

  refresh(): void {
    this.emitter.fire(undefined);
  }

  getTreeItem(el: RunItem): vscode.TreeItem {
    // Override the command to use our custom log viewer.
    el.command = {
      command: 'vscode.open',
      title: 'Open Shard log (ANSI-stripped)',
      arguments: [LogFileProvider.asUri(el.run.log_path)],
    };
    return el;
  }

  async getChildren(el?: RunItem): Promise<RunItem[]> {
    if (el) return [];
    const rows = await this.reader.recent(30);
    return rows.map(r => new RunItem(r));
  }
}

class RunItem extends vscode.TreeItem {
  constructor(public readonly run: HistoryRow) {
    const head = run.argv[0] || 'unknown';
    const rest = run.argv.slice(1).join(' ');
    super(`${head} ${rest}`.trim().slice(0, 64), vscode.TreeItemCollapsibleState.None);
    const pct = run.savings_pct.toFixed(1);
    this.description = `-${pct}%  ${run.exit_code === 0 ? '✓' : `✗${run.exit_code}`} ${run.wall_ms}ms`;
    this.tooltip = new vscode.MarkdownString(
      `**${head} ${rest}**\n\n` +
        `Started: ${run.started_at}\n\n` +
        `Tokens: ${run.tokens_in} → ${run.tokens_out} (**-${pct}%**)\n\n` +
        `Wall: ${run.wall_ms}ms · Exit: ${run.exit_code}\n\n` +
        `Log: \`${run.log_path}\``
    );
    this.iconPath = new vscode.ThemeIcon(
      run.savings_pct >= 60 ? 'arrow-up'
      : run.savings_pct >= 20 ? 'circle-filled'
      : 'circle-outline'
    );
    this.contextValue = 'shardRun';
    // Command is set dynamically in getTreeItem to use LogFileProvider
  }
}
