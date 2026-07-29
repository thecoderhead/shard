import * as vscode from 'vscode';
import * as path from 'path';
import * as fs from 'fs';

export interface RunSummary {
  total_commands: number;
  tokens_in: number;
  tokens_out: number;
  tokens_saved: number;
  savings_pct: number;
  total_exec_ms: number;
  avg_exec_ms: number;
}
export interface HistoryRow {
  id: string; started_at: string; argv: string[];
  savings_pct: number; tokens_saved: number;
  tokens_in: number; tokens_out: number;
  wall_ms: number; exit_code: number; log_path: string;
}
export interface DailyRow {
  day: string; runs: number; tokens_in: number; tokens_out: number;
}
export interface MetricsSnapshot {
  summary: RunSummary; recent: HistoryRow[];
  daily: DailyRow[];
  topCommands: { command: string; runs: number; saved: number; pct: number }[];
}

export class MetricsReader implements vscode.Disposable {
  private roots: string[] = [];
  private watchers: fs.FSWatcher[] = [];
  private changeEmitter = new vscode.EventEmitter<void>();
  private pollTimer: ReturnType<typeof setInterval> | undefined;
  private sqlitialized = false;
  private sqlInit: any = null;
  private barrier: Promise<void>;
  private barrierResolve!: () => void;
  /** Count polls so we periodically re-scan when no DB is found yet. */
  private pollsWithNoRoots = 0;
  private _lastMtime = 0;
  readonly onChange = this.changeEmitter.event;

  constructor() {
    this.barrier = new Promise(r => { this.barrierResolve = r; });
    this.init();
    this.rescan();
    this.pollTimer = setInterval(() => this.checkChanges(), 2000);
  }

  private async init() {
    try {
      const initSqlJs = require('sql.js');
      this.sqlInit = await initSqlJs();
      this.sqlitialized = true;
    } catch (e: any) {
      console.error('Shard metrics: sql.js load failed', e.message);
    }
    this.barrierResolve();
  }

  private dbPath(): string | null {
    // 1) Explicit config overrides everything.
    const cfgPath = vscode.workspace.getConfiguration('shard').get<string>('metricsDbPath', '');
    if (cfgPath && fs.existsSync(cfgPath)) return cfgPath;
    // 2) Look in workspace .shard/ directories.
    for (const r of this.roots) {
      const p = path.join(r, 'metrics.db');
      if (fs.existsSync(p)) return p;
    }
    return null;
  }

  /**
   * Open the DB, run `fn(db)`, close.  Single read + single sql.js
   * instantiation per call — avoids loading the full SQLite file into
   * WASM heap N times.
   */
  private withDb<T>(fn: (db: any) => T): T | null {
    const dbp = this.dbPath();
    if (!dbp) return null;
    try {
      const data = fs.readFileSync(dbp);
      const db = new this.sqlInit.Database(data);
      try {
        return fn(db);
      } finally {
        db.close();
      }
    } catch (e) {
      console.error('Shard metrics: query failed', e);
      return null;
    }
  }

  /** Execute a prepared statement against an already-open DB. */
  private _query<T>(db: any, sql: string, params: any[], map: (r: any) => T): T[] {
    const stmt = db.prepare(sql);
    try {
      if (params.length) stmt.bind(params);
      const out: T[] = [];
      while (stmt.step()) out.push(map(stmt.getAsObject()));
      return out;
    } finally {
      stmt.free();
    }
  }

  candidateRoots(): string[] { return [...this.roots]; }

  rescan(): void {
    for (const w of this.watchers) { try { w.close(); } catch {} }
    this.watchers = [];
    this.roots = [];
    const folders = vscode.workspace.workspaceFolders;
    if (!folders) return;
    for (const f of folders) {
      const cand = path.join(f.uri.fsPath, '.shard');
      if (fs.existsSync(path.join(cand, 'metrics.db'))) {
        this.roots.push(cand);
        const dbp = path.join(cand, 'metrics.db');
        try {
          const w = fs.watch(cand, { persistent: false }, (_evt, fn) => {
            if (fn && (fn === 'metrics.db' || fn === 'metrics.db-wal' || fn === 'metrics.db-shm')) {
              this.changeEmitter.fire();
            }
          });
          this.watchers.push(w);
        } catch {}
      }
    }
  }

  private checkChanges(): void {
    // If no DB roots have ever been found, periodically re-scan so we
    // pick up a metrics.db that appears after the extension activates.
    if (this.roots.length === 0) {
      this.pollsWithNoRoots++;
      if (this.pollsWithNoRoots % 10 === 0) this.rescan();
      return;
    }
    // Only fire change event if DB mtime actually changed.
    for (const r of this.roots) {
      const dbp = path.join(r, 'metrics.db');
      if (!fs.existsSync(dbp)) continue;
      const mtime = fs.statSync(dbp).mtimeMs;
      if (mtime !== this._lastMtime) {
        this._lastMtime = mtime;
        this.changeEmitter.fire();
      }
      return;
    }
  }

  // ---- single-query helpers used both standalone and by snapshot() ----

  async summary(): Promise<RunSummary | null> {
    await this.barrier;
    if (!this.sqlitialized) return null;
    return this.withDb(db => this._summaryFromDb(db));
  }

  private _summaryFromDb(db: any): RunSummary | null {
    const rows = this._query(db,
      'SELECT COUNT(*) AS total, COALESCE(SUM(tokens_in),0) AS ti, COALESCE(SUM(tokens_out),0) AS to_, COALESCE(SUM(wall_ms),0) AS wall FROM runs',
      [], r => r);
    if (!rows.length) return null;
    const total = Number(rows[0].total);
    const ti = Number(rows[0].ti);
    const to_ = Number(rows[0].to_);
    const wall = Number(rows[0].wall);
    const saved = Math.max(0, ti - to_);
    return {
      total_commands: total, tokens_in: ti, tokens_out: to_,
      tokens_saved: saved, savings_pct: ti === 0 ? 0 : (saved / ti) * 100,
      total_exec_ms: wall, avg_exec_ms: total === 0 ? 0 : Math.floor(wall / total),
    };
  }

  async recent(limit = 30): Promise<HistoryRow[]> {
    await this.barrier;
    if (!this.sqlitialized) return [];
    return this.withDb(db => this._recentFromDb(db, limit)) ?? [];
  }

  private _recentFromDb(db: any, limit: number): HistoryRow[] {
    return this._query(db,
      'SELECT id,started_at,argv,savings_pct,tokens_in,tokens_out,wall_ms,exit_code,log_path FROM runs ORDER BY started_at DESC LIMIT ?',
      [limit],
      r => {
        let argv: string[] = [];
        try { argv = JSON.parse(r.argv); } catch { argv = [r.argv || '']; }
        return {
          id: r.id ?? '', started_at: r.started_at ?? '', argv,
          savings_pct: Number(r.savings_pct),
          tokens_saved: Math.max(0, Number(r.tokens_in) - Number(r.tokens_out)),
          tokens_in: Number(r.tokens_in), tokens_out: Number(r.tokens_out),
          wall_ms: Number(r.wall_ms), exit_code: Number(r.exit_code),
          log_path: r.log_path ?? '',
        };
      });
  }

  async daily(days = 30): Promise<DailyRow[]> {
    await this.barrier;
    if (!this.sqlitialized) return [];
    return this.withDb(db => this._dailyFromDb(db, days)) ?? [];
  }

  private _dailyFromDb(db: any, days: number): DailyRow[] {
    const cutoff = new Date(Date.now() - days * 86400000).toISOString().slice(0, 10);
    return this._query(db,
      'SELECT DATE(started_at) AS day, COUNT(*) AS runs, COALESCE(SUM(tokens_in),0) AS ti, COALESCE(SUM(tokens_out),0) AS to_ FROM runs WHERE started_at >= ? GROUP BY day ORDER BY day ASC',
      [cutoff],
      r => ({
        day: r.day ?? '', runs: Number(r.runs),
        tokens_in: Number(r.ti), tokens_out: Number(r.to_),
      }));
  }

  async topCommands(limit = 10) {
    await this.barrier;
    if (!this.sqlitialized) return [];
    return this.withDb(db => this._topCommandsFromDb(db, limit)) ?? [];
  }

  private _topCommandsFromDb(db: any, limit: number) {
    const rows = this._query(db,
      'SELECT argv, COALESCE(SUM(tokens_in),0) AS ti, COALESCE(SUM(tokens_out),0) AS to_, COUNT(*) AS cnt FROM runs GROUP BY argv ORDER BY cnt DESC LIMIT ?',
      [limit], r => r);
    const buckets = new Map<string, { runs: number; ti: number; to: number }>();
    for (const row of rows) {
      let head = 'unknown';
      try { const a = JSON.parse(row.argv); head = Array.isArray(a) && a.length > 0 ? a[0] : 'unknown'; } catch {}
      const b = buckets.get(head) || { runs: 0, ti: 0, to: 0 };
      b.runs += Number(row.cnt); b.ti += Number(row.ti); b.to += Number(row.to_);
      buckets.set(head, b);
    }
    return Array.from(buckets.entries()).map(([cmd, b]) => ({
      command: cmd, runs: b.runs, saved: Math.max(0, b.ti - b.to),
      pct: b.ti === 0 ? 0 : ((b.ti - b.to) / b.ti) * 100,
    })).sort((a, b) => b.saved - a.saved).slice(0, limit);
  }

  // ---- snapshot: single DB load for all queries ----

  async snapshot(): Promise<MetricsSnapshot> {
    await this.barrier;
    const defSum: RunSummary = { total_commands: 0, tokens_in: 0, tokens_out: 0, tokens_saved: 0, savings_pct: 0, total_exec_ms: 0, avg_exec_ms: 0 };
    if (!this.sqlitialized) return { summary: defSum, recent: [], daily: [], topCommands: [] };
    const result = this.withDb(db => ({
      summary: this._summaryFromDb(db),
      recent: this._recentFromDb(db, 30),
      daily: this._dailyFromDb(db, 30),
      topCommands: this._topCommandsFromDb(db, 10),
    }));
    if (!result) return { summary: defSum, recent: [], daily: [], topCommands: [] };
    return {
      summary: result.summary ?? defSum,
      recent: result.recent,
      daily: result.daily,
      topCommands: result.topCommands,
    };
  }

  dispose(): void {
    if (this.pollTimer) clearInterval(this.pollTimer);
    for (const w of this.watchers) { try { w.close(); } catch {} }
    this.watchers = [];
    this.changeEmitter.dispose();
  }
}
