import * as vscode from 'vscode';
import * as path from 'path';
import * as fs from 'fs';
import { MetricsReader, RunSummary, HistoryRow, DailyRow } from './metrics';
import { DashboardPanel } from './dashboard';
import { RecentRunsProvider } from './treeView';
import { SummaryViewProvider } from './sidebarView';
import { LogFileProvider } from './logViewer';

// Model pricing lookup ($ per 1K tokens).
const MODEL_PRICES: Record<string, { input: number; output: number }> = {
  'claude-sonnet-4': { input: 3.0, output: 15.0 },
  'claude-opus-4': { input: 15.0, output: 75.0 },
  'gpt-4o': { input: 2.5, output: 10.0 },
  'gpt-4.1': { input: 2.0, output: 8.0 },
  'gpt-4.1-mini': { input: 0.4, output: 1.6 },
  'gpt-4.1-nano': { input: 0.1, output: 0.4 },
};

export function getModelPrice(model: string): { input: number; output: number } {
  const known = MODEL_PRICES[model];
  if (known) return known;
  const cfg = vscode.workspace.getConfiguration('shard');
  return {
    input: cfg.get<number>('modelPricePer1kInput', 3.0),
    output: cfg.get<number>('modelPricePer1kOutput', 15.0),
  };
}

/**
 * Extension activation entry point.
 *
 * Wires up:
 *  - MetricsReader: SQLite reader with fs.watch-based change detection.
 *  - StatusBarItem: live "Shard -X% saved" with pulse animation on new data.
 *  - Cost-in-dollars: optional model-based cost savings display.
 *  - Tree view: recent runs list in the Explorer sidebar.
 *  - DashboardPanel: full webview with charts + cost view.
 */
export function activate(context: vscode.ExtensionContext) {
  const reader = new MetricsReader();
  context.subscriptions.push(reader);

  const statusBarItem = vscode.window.createStatusBarItem(vscode.StatusBarAlignment.Right, 100);
  statusBarItem.command = 'shard.showDashboard';
  statusBarItem.tooltip = '⫸ SHARD HUD ⫷  —  click to open dashboard';
  statusBarItem.hide();
  context.subscriptions.push(statusBarItem);

  const treeProvider = new RecentRunsProvider(reader);
  context.subscriptions.push(
    vscode.window.createTreeView('shard.recentRuns', { treeDataProvider: treeProvider, showCollapseAll: false })
  );

  // Register the custom log viewer for .shard/logs/*.log files.
  const logProvider = new LogFileProvider();
  context.subscriptions.push(
    vscode.workspace.registerTextDocumentContentProvider(LogFileProvider.scheme, logProvider)
  );

  // Override the tree view click to open via our provider.
  treeProvider.onDidOpenLog(logPath => {
    vscode.commands.executeCommand('vscode.open', LogFileProvider.asUri(logPath));
  });

  const summaryProvider = new SummaryViewProvider(context.extensionUri, reader);
  context.subscriptions.push(
    vscode.window.registerWebviewViewProvider(SummaryViewProvider.viewType, summaryProvider, {
      webviewOptions: { retainContextWhenHidden: true },
    })
  );

  let lastStatusPct = 0;
  let pulseTimer: ReturnType<typeof setTimeout> | undefined;

  const refreshUi = async (isWatchEvent = false) => {
    try {
      const snap = await reader.snapshot();
      const summary = snap.summary;
      const hasData = summary.total_commands > 0;
      vscode.commands.executeCommand('setContext', 'shard.hasMetrics', hasData);

      if (hasData && getConfig<boolean>('statusBar.enabled', true)) {
        const pct = summary.savings_pct.toFixed(1);
        const saved = summary.tokens_saved.toLocaleString('en-US');
        const model = getConfig<string>('model', 'claude-sonnet-4');
        const prices = getModelPrice(model);
        const costSaved = ((summary.tokens_saved / 1000) * (prices.input + prices.output) / 2).toFixed(2);

        // Pulse animation on watch-triggered changes.
        if (isWatchEvent && summary.savings_pct !== lastStatusPct) {
          statusBarItem.text = `$(debug-rerun) SHARD ${pct}%`;
          if (pulseTimer) clearTimeout(pulseTimer);
          pulseTimer = setTimeout(() => { statusBarItem.text = `$(pulse) SHARD ${pct}%`; }, 2000);
        } else {
          statusBarItem.text = `$(pulse) SHARD ${pct}%`;
        }
        lastStatusPct = summary.savings_pct;

        statusBarItem.tooltip = new vscode.MarkdownString(
          `**⫸ SHARD HUD ⫷**  \n` +
          `Total runs: **${summary.total_commands}**  \n` +
          `Tokens saved: **${saved}** (${pct}%)  \n` +
          `Cost saved: **$${costSaved}** (${model})  \n` +
          `Avg exec: ${summary.avg_exec_ms}ms  \n\n` +
          `Click to open the HUD dashboard.`
        );
        statusBarItem.show();
      } else {
        statusBarItem.hide();
      }

      treeProvider.refresh();
      const model = getConfig<string>('model', 'claude-sonnet-4');
      const prices = getModelPrice(model);
      (snap as any)._model = model;
      (snap as any)._priceInput = prices.input;
      (snap as any)._priceOutput = prices.output;
      DashboardPanel.currentPanel?.pushUpdate(snap);
      summaryProvider.pushUpdate(snap);
    } catch (err) {
      console.error(`Shard: refresh failed: ${err}`);
    }
  };

  // --- Commands ---
  context.subscriptions.push(
    vscode.commands.registerCommand('shard.showDashboard', () =>
      DashboardPanel.createOrShow(context.extensionUri, reader)),
    vscode.commands.registerCommand('shard.refreshMetrics', () => refreshUi()),
    vscode.commands.registerCommand('shard.installHooks', async () => {
      const terminal = vscode.window.createTerminal('Shard: install hooks');
      terminal.sendText('shard init -g');
      terminal.show();
    }),
    vscode.commands.registerCommand('shard.revealLogs', async () => {
      const roots = reader.candidateRoots();
      if (!roots.length) {
        vscode.window.showInformationMessage('No .shard/ directory found in workspace.');
        return;
      }
      const logsDir = path.join(roots[0], 'logs');
      if (!fs.existsSync(logsDir)) {
        vscode.window.showInformationMessage(`No logs directory yet at ${logsDir}`);
        return;
      }
      vscode.commands.executeCommand('revealFileInOS', vscode.Uri.file(logsDir));
    }),
    vscode.commands.registerCommand('shard.clean', async () => {
      const terminal = vscode.window.createTerminal('Shard: clean');
      terminal.sendText('shard clean');
      terminal.show();
    }),
    vscode.commands.registerCommand('shard.rebuildNative', async () => {
      const action = await vscode.window.showInformationMessage(
        'Rebuild better-sqlite3 native module for VS Code\'s Electron ABI?',
        { modal: true },
        'Rebuild Now'
      );
      if (action !== 'Rebuild Now') return;
      const terminal = vscode.window.createTerminal('Shard: rebuild native');
      terminal.sendText(`cd "${context.extensionPath}" && npm rebuild better-sqlite3`);
      terminal.show();
    })
  );

  // --- Poll + watch loop ---
  reader.onChange(() => refreshUi(true));
  const pollMs = getConfig<number>('pollIntervalMs', 3000);
  const timer = setInterval(() => refreshUi(), pollMs);
  context.subscriptions.push({ dispose: () => clearInterval(timer) });

  // --- Config / workspace change handlers ---
  context.subscriptions.push(
    vscode.workspace.onDidChangeConfiguration(evt => {
      if (evt.affectsConfiguration('shard')) { refreshUi(); }
    }),
    vscode.workspace.onDidChangeWorkspaceFolders(() => {
      reader.rescan();
      refreshUi();
    })
  );

  refreshUi();

  if (getConfig<boolean>('dashboard.openOnStartup', false)) {
    reader.summary().then(s => {
      if (s && s.total_commands > 0) DashboardPanel.createOrShow(context.extensionUri, reader);
    });
  }
}

export function deactivate() {}

function getConfig<T>(key: string, def: T): T {
  const cfg = vscode.workspace.getConfiguration('shard');
  const v = cfg.get<T>(key);
  return v ?? def;
}

export type { RunSummary, HistoryRow, DailyRow };
