# Shard — VS Code Extension

Real-time GUI for `shard`'s token-savings metrics.

Reads `.shard/metrics.db` in each workspace folder, watches it for changes, and renders:

- **Status bar** — running `-XX.X% saved` badge with a Markdown tooltip breakdown.
- **Explorer tree view** — 30 most recent runs, click to open the raw log file.
- **Full dashboard webview** — totals cards, daily-savings bar chart, top-commands leaderboard, scrollable recent-runs table. Live-updates as new commands run.

## Why this indirect design

Copilot (and every other AI coding agent) runs shell commands in an **out-of-process subprocess** it launches itself. There is no in-process hook the extension can attach to. Shard installs a shell-alias hook (`shard init -g`) so that when Copilot runs `git status` under the hood, the shell resolves the alias to `shard git status`, Shard's binary intercepts + compacts the output, and one row per invocation lands in `.shard/metrics.db`. This extension then reads that DB — completely decoupled from Copilot's own process.

## Install (dev mode)

```powershell
cd extension
npm install
npm run compile
# In VS Code:  F5 (Extension Development Host)
# Or CLI:      code --extensionDevelopmentPath=(Resolve-Path .).Path
```

## Configuration

| Setting | Default | Description |
| --- | --- | --- |
| `shard.metricsDbPath` | `""` | Absolute path to `metrics.db`. Empty = auto-discover from workspace folders. |
| `shard.pollIntervalMs` | `1500` | Fallback poll interval when native fs watching stalls. |
| `shard.statusBar.enabled` | `true` | Show the running savings percentage in the status bar. |
| `shard.dashboard.openOnStartup` | `false` | Auto-open the full dashboard on activation. |

## Layout

```
extension/
├── package.json         VS Code contribution manifest
├── tsconfig.json
└── src/
    ├── extension.ts     activation, commands, status bar, poll loop
    ├── metrics.ts       better-sqlite3 reader + fs.watch change stream
    ├── treeView.ts      Explorer sidebar recent-runs list
    └── dashboard.ts     full webview dashboard (custom HTML, no external CDN)
```

## `better-sqlite3` note

`better-sqlite3` is a native module. VS Code's bundled Electron ABI may not match the one npm installs against on your machine. If activation fails with `MODULE_NOT_FOUND`, rebuild it:

```powershell
cd extension
npm rebuild better-sqlite3
# or against Electron specifically:
npx @electron/rebuild -v <electron-version> -w better-sqlite3
```

If native rebuild is a pain, swap `metrics.ts` to spawn `shard gain --format json` as a subprocess — slower but zero native dep. That fallback is on the roadmap.
