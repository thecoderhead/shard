import * as vscode from 'vscode';
import { MetricsReader, MetricsSnapshot } from './metrics';

export class DashboardPanel {
  static currentPanel: DashboardPanel | undefined;
  private static readonly viewType = 'shard.dashboard';
  private readonly panel: vscode.WebviewPanel;
  private disposables: vscode.Disposable[] = [];

  static createOrShow(extensionUri: vscode.Uri, reader: MetricsReader) {
    const column = vscode.window.activeTextEditor?.viewColumn;
    if (DashboardPanel.currentPanel) {
      DashboardPanel.currentPanel.panel.reveal(column);
      return;
    }
    const panel = vscode.window.createWebviewPanel(
      DashboardPanel.viewType,
      'SHARD — Metrics Dashboard',
      column || vscode.ViewColumn.One,
      { enableScripts: true, retainContextWhenHidden: true, localResourceRoots: [vscode.Uri.joinPath(extensionUri, 'media')] }
    );
    DashboardPanel.currentPanel = new DashboardPanel(panel, reader);
  }

  private constructor(panel: vscode.WebviewPanel, private readonly reader: MetricsReader) {
    this.panel = panel;
    this.panel.webview.html = renderHtml();
    this.panel.onDidDispose(() => this.dispose(), null, this.disposables);
    this.panel.webview.onDidReceiveMessage(async msg => {
      if (msg?.type === 'ready' || msg?.type === 'refresh') {
        const snap = await this.reader.snapshot();
        const model = vscode.workspace.getConfiguration('shard').get<string>('model', 'claude-sonnet-4');
        const prices = extensionGetModelPrice(model);
        (snap as any)._model = model;
        (snap as any)._priceInput = prices.input;
        (snap as any)._priceOutput = prices.output;
        (snap as any)._showCost = vscode.workspace.getConfiguration('shard').get<boolean>('dashboard.showCost', true);
        this.pushUpdate(snap);
      } else if (msg?.type === 'openLog' && typeof msg.path === 'string') {
        vscode.commands.executeCommand('vscode.open', vscode.Uri.file(msg.path));
      } else if (msg?.type === 'installHooks') {
        vscode.commands.executeCommand('shard.installHooks');
      } else if (msg?.type === 'revealLogs') {
        vscode.commands.executeCommand('shard.revealLogs');
      } else if (msg?.type === 'exportCSV') {
        const snap = await this.reader.snapshot();
        const csv = toCsv(snap);
        vscode.env.clipboard.writeText(csv);
        vscode.window.showInformationMessage('Shard: CSV copied to clipboard!');
      }
    });
  }

  pushUpdate(snapshot: MetricsSnapshot) {
    this.panel.webview.postMessage({ type: 'update', snapshot });
  }

  private dispose() {
    DashboardPanel.currentPanel = undefined;
    this.panel.dispose();
    while (this.disposables.length) { const d = this.disposables.pop(); d?.dispose(); }
  }
}

function toCsv(snap: MetricsSnapshot): string {
  const recent = snap.recent || [];
  const lines = ['started_at,argv,savings_pct,tokens_in,tokens_out,tokens_saved,wall_ms,exit_code'];
  for (const r of recent) {
    const cmd = (r.argv || []).join(' ');
    const saved = Math.max(0, r.tokens_in - r.tokens_out);
    lines.push(`"${r.started_at}","${cmd}",${r.savings_pct.toFixed(1)},${r.tokens_in},${r.tokens_out},${saved},${r.wall_ms},${r.exit_code}`);
  }
  return lines.join('\n');
}

function extensionGetModelPrice(model: string): { input: number; output: number } {
  const prices: Record<string, { input: number; output: number }> = {
    'claude-sonnet-4': { input: 3.0, output: 15.0 },
    'claude-opus-4': { input: 15.0, output: 75.0 },
    'gpt-4o': { input: 2.5, output: 10.0 },
    'gpt-4.1': { input: 2.0, output: 8.0 },
    'gpt-4.1-mini': { input: 0.4, output: 1.6 },
    'gpt-4.1-nano': { input: 0.1, output: 0.4 },
  };
  const known = prices[model];
  if (known) return known;
  const cfg = vscode.workspace.getConfiguration('shard');
  return { input: cfg.get<number>('modelPricePer1kInput', 3.0), output: cfg.get<number>('modelPricePer1kOutput', 15.0) };
}

function renderHtml(): string {
  const nonce = mkNonce();
  return /* html */`<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="UTF-8" />
<meta http-equiv="Content-Security-Policy"
      content="default-src 'none'; style-src 'unsafe-inline'; script-src 'nonce-${nonce}';">
<title>SHARD DASHBOARD</title>
<style>
  :root {
    --bg-deep:    #111112;
    --bg-glass:   rgba(30, 30, 32, 0.6);
    --glass-border:     rgba(255, 255, 255, 0.05);
    --glass-border-hi:  rgba(255, 255, 255, 0.09);
    --glass-shine:      rgba(255, 255, 255, 0.025);
    --glass-reflect:    linear-gradient(135deg, rgba(255,255,255,0.05) 0%, transparent 50%);
    --accent-gold:      #c9a45c;
    --accent-sage:      #8a9b8e;
    --accent-warm:      #b8957a;
    --accent-steel:     #8899aa;
    --accent-muted:     #a0978c;
    --grad-gold:        linear-gradient(135deg, #c9a45c, #b8957a);
    --grad-sage:        linear-gradient(135deg, #8a9b8e, #a0978c);
    --glow-gold:   0 0 24px rgba(201, 164, 92, 0.06);
    --glow-subtle: 0 0 8px rgba(255, 255, 255, 0.015);
    --text-primary:   #e8e6e1;
    --text-secondary: #b0aba4;
    --text-tertiary:  #7a7670;
    --font-display: system-ui, -apple-system, 'Segoe UI', Roboto, Helvetica, Arial, sans-serif;
    --font-mono:  var(--vscode-editor-font-family, 'JetBrains Mono', 'Cascadia Code', 'Fira Code', 'Consolas', monospace);
    --radius-sm:  4px;
    --radius-md:  8px;
    --radius-lg:  12px;
    --radius-xl:  16px;
  }
  * { box-sizing: border-box; margin: 0; padding: 0; }
  html, body {
    background: var(--bg-deep);
    color: var(--text-primary);
    font-family: var(--font-display);
    font-size: 12px; line-height: 1.5;
    overflow: hidden;
  }
  body::before {
    content: ''; position: fixed; top: 0; left: 0; right: 0; height: 50%;
    background: radial-gradient(ellipse at 70% -10%, rgba(201,164,92,.04) 0%, transparent 70%);
    pointer-events: none; z-index: 0;
  }
  .app { display: flex; flex-direction: column; height: 100vh; position: relative; z-index: 1; }

  /* ── Glass card ── */
  .glass {
    background: var(--bg-glass);
    backdrop-filter: blur(16px) saturate(1.5);
    -webkit-backdrop-filter: blur(16px) saturate(1.5);
    border: 1px solid var(--glass-border);
    border-radius: var(--radius-lg);
    box-shadow: var(--glow-subtle), inset 0 1px 0 var(--glass-shine);
    position: relative;
    overflow: hidden;
    transition: border-color .35s ease, box-shadow .35s ease;
  }
  .glass::before {
    content: ''; position: absolute; top: 0; left: 0; right: 0; height: 1px;
    background: var(--glass-reflect); pointer-events: none;
  }
  .glass:hover { border-color: var(--glass-border-hi); box-shadow: var(--glow-subtle), var(--glow-gold), inset 0 1px 0 var(--glass-shine); }
  .glass.accent-top::after {
    content: ''; position: absolute; top: 0; left: 1px; right: 1px; height: 1px;
    border-radius: var(--radius-lg) var(--radius-lg) 0 0;
    background: var(--grad-gold); opacity: .4;
  }

  /* ── Top bar ── */
  .topbar {
    display: flex; align-items: center; justify-content: space-between;
    padding: 10px 16px; flex-shrink: 0;
    background: var(--bg-glass); backdrop-filter: blur(16px) saturate(1.5);
    -webkit-backdrop-filter: blur(16px) saturate(1.5);
    border-bottom: 1px solid var(--glass-border);
  }
  .brand { display: flex; align-items: center; gap: 10px; }
  .brand-icon {
    display: flex; align-items: center; justify-content: center;
    width: 22px; height: 22px; border-radius: var(--radius-sm);
    background: var(--grad-gold); color: #111;
    font-size: 11px; font-weight: 700; font-family: var(--font-mono);
  }
  .brand-name { font-size: 13px; font-weight: 600; letter-spacing: .04em; color: var(--text-primary); }
  .brand-name span { color: var(--accent-gold); }
  .brand-sub { font-size: 10px; color: var(--text-tertiary); font-weight: 400; margin-left: 6px; }
  .topbar-actions { display: flex; gap: 6px; }
  .btn {
    padding: 5px 12px; border: 1px solid var(--glass-border);
    border-radius: 99px; cursor: pointer; font-family: var(--font-display);
    font-size: 10px; font-weight: 500; letter-spacing: .02em;
    background: transparent; color: var(--text-secondary);
    transition: all .2s ease; white-space: nowrap;
    display: flex; align-items: center; gap: 4px;
  }
  .btn:hover { background: rgba(255,255,255,.04); border-color: var(--glass-border-hi); color: var(--text-primary); }
  .btn.primary { background: rgba(201,164,92,.08); border-color: rgba(201,164,92,.2); color: var(--accent-gold); }
  .btn.primary:hover { background: rgba(201,164,92,.12); border-color: rgba(201,164,92,.3); }

  /* ── Scrollable content ── */
  .content { flex: 1; overflow-y: auto; padding: 14px 16px; }

  /* ── Section header ── */
  .section-hdr { display: flex; align-items: center; gap: 8px; margin-bottom: 12px; }
  .section-hdr .dot { width: 5px; height: 5px; border-radius: 50%; background: var(--accent-gold); flex-shrink: 0; }
  .section-hdr h2 { font-size: 10px; font-weight: 600; text-transform: uppercase; letter-spacing: .08em; color: var(--text-secondary); margin: 0; }
  .section-hdr .line { flex: 1; height: 0; border-top: 1px solid rgba(255,255,255,.04); }

  /* ── KPI grid ── */
  .kpi-grid { display: grid; grid-template-columns: repeat(3, 1fr); gap: 8px; margin-bottom: 14px; }
  .kpi {
    padding: 14px; border-radius: var(--radius-lg);
    background: var(--bg-glass); backdrop-filter: blur(16px) saturate(1.5);
    -webkit-backdrop-filter: blur(16px) saturate(1.5);
    border: 1px solid var(--glass-border);
    transition: border-color .35s ease, box-shadow .35s ease;
  }
  .kpi:hover { border-color: var(--glass-border-hi); box-shadow: var(--glow-gold); }
  .kpi .lbl { font-size: 9px; font-weight: 500; text-transform: uppercase; letter-spacing: .08em; color: var(--text-tertiary); margin-bottom: 4px; }
  .kpi .val { font-size: 20px; font-weight: 600; color: var(--text-primary); font-family: var(--font-mono); font-variant-numeric: tabular-nums; line-height: 1.2; }
  .kpi .sub { font-size: 10px; color: var(--text-tertiary); margin-top: 3px; font-weight: 400; }
  .kpi .bar-track { height: 2px; background: rgba(255,255,255,.04); margin-top: 8px; overflow: hidden; border-radius: 99px; }
  .kpi .bar-fill { height: 100%; border-radius: 99px; transition: width .6s ease; }
  .kpi .bar-fill.gold { background: var(--grad-gold); }
  .kpi .bar-fill.sage { background: var(--grad-sage); }
  .kpi.accent-gold .val { color: var(--accent-gold); }
  .kpi.accent-sage .val { color: var(--accent-sage); }
  .kpi.accent-warm .val { color: var(--accent-warm); }
  .kpi.accent-steel .val { color: var(--accent-steel); }
  .kpi.accent-muted .val { color: var(--accent-muted); }

  /* ── Row / col layout ── */
  .row { display: flex; gap: 12px; margin-bottom: 12px; }
  .col { flex: 1; min-width: 280px; display: flex; flex-direction: column; gap: 12px; }

  /* ── Daily savings chart ── */
  .chart-wrap { padding: 12px 12px 8px; }
  .chart { display: flex; align-items: flex-end; gap: 2px; height: 80px; padding-top: 4px; }
  .chart .day { flex: 1; min-width: 4px; display: flex; flex-direction: column-reverse; align-items: center; gap: 1px; }
  .chart .day .bar { width: 100%; min-height: 2px; border-radius: 2px 2px 0 0; transition: height 400ms ease; background: var(--grad-gold); }
  .chart .day .lbl { font-size: 7px; color: var(--text-tertiary); font-family: var(--font-mono); }
  .chart .day .val { font-size: 7px; color: var(--text-tertiary); font-family: var(--font-mono); }

  /* ── Weekly ── */
  .weekly { display: flex; gap: 12px; padding: 12px; flex-wrap: wrap; }
  .weekly .stat { flex: 1; min-width: 80px; }
  .weekly .stat .wlbl { font-size: 9px; color: var(--text-tertiary); text-transform: uppercase; letter-spacing: .06em; font-weight: 500; }
  .weekly .stat .wval { font-size: 18px; font-weight: 700; font-family: var(--font-mono); color: var(--text-primary); margin: 3px 0; font-variant-numeric: tabular-nums; }
  .weekly .stat .wsub { font-size: 10px; color: var(--text-tertiary); }
  .change-up { color: var(--accent-sage); }
  .change-down { color: var(--accent-warm); }

  /* ── Archetype donut ── */
  .donut-wrap { display: flex; align-items: center; gap: 14px; padding: 12px; }
  .donut { width: 72px; height: 72px; border-radius: 50%; flex-shrink: 0; box-shadow: 0 0 24px rgba(201,164,92,.06); }
  .donut-legend { font-size: 10px; font-family: var(--font-mono); list-style: none; flex: 1; }
  .donut-legend li { margin: 3px 0; display: flex; align-items: center; gap: 5px; }
  .donut-legend .swatch { width: 7px; height: 7px; border-radius: 50%; flex-shrink: 0; }

  /* ── Tables ── */
  table { width: 100%; border-collapse: collapse; font-family: var(--font-mono); font-size: 10px; }
  thead { position: relative; }
  thead::after { content: ''; position: absolute; bottom: 0; left: 0; right: 0; height: 1px; background: rgba(255,255,255,.04); }
  th, td { padding: 5px 8px; text-align: left; }
  th { color: var(--text-tertiary); font-weight: 500; font-size: 9px; text-transform: uppercase; letter-spacing: .06em; font-family: var(--font-display); }
  tr td { border-bottom: 1px solid rgba(255,255,255,.02); }
  tr:last-child td { border-bottom: none; }
  tr:hover td { background: rgba(255,255,255,.02); cursor: pointer; }

  /* ── Color utilities ── */
  .gold  { color: var(--accent-gold); }
  .sage  { color: var(--accent-sage); }
  .warm  { color: var(--accent-warm); }
  .steel { color: var(--accent-steel); }
  .muted { color: var(--accent-muted); }
  .dim   { color: var(--text-tertiary); }
  .empty { color: var(--text-tertiary); padding: 12px 0; text-align: center; }

  /* ── Efficiency badge ── */
  .eff-badge {
    display: inline-flex; align-items: center; gap: 4px;
    padding: 1px 7px; border-radius: 99px;
    font-size: 9px; font-weight: 400; font-family: var(--font-display);
  }
  .eff-badge.high { background: rgba(201,164,92,.08); border: 1px solid rgba(201,164,92,.15); color: var(--accent-gold); }
  .eff-badge.mid  { background: rgba(138,155,142,.08); border: 1px solid rgba(138,155,142,.15); color: var(--accent-sage); }
  .eff-badge.low  { background: rgba(184,149,122,.08); border: 1px solid rgba(184,149,122,.15); color: var(--accent-warm); }

  /* ── Footer ── */
  .footer {
    border-top: 1px solid var(--glass-border);
    padding: 6px 16px; flex-shrink: 0;
    font-size: 9px; color: var(--text-tertiary);
    letter-spacing: .02em;
    display: flex; justify-content: space-between;
    background: var(--bg-glass);
    backdrop-filter: blur(12px); -webkit-backdrop-filter: blur(12px);
  }
  .footer .dot { display: inline-block; width: 4px; height: 4px; border-radius: 50%; background: var(--accent-sage); margin-right: 4px; vertical-align: middle; }

  /* ── Animations ── */
  @keyframes fadeSlideIn {
    from { opacity: 0; transform: translateY(6px); }
    to   { opacity: 1; transform: translateY(0); }
  }
  .anim-in { animation: fadeSlideIn .35s ease both; }
  
  @keyframes countPulse {
    0%   { transform: scale(1); }
    50%  { transform: scale(1.06); }
    100% { transform: scale(1); }
  }
  .val.flash { animation: countPulse .35s ease; }
</style>
</head>
<body>
<div class="app">
  <div class="topbar">
    <div class="brand">
      <div class="brand-icon">◆</div>
      <div class="brand-name"><span>SHARD</span> <span class="brand-sub">Metrics Dashboard · v0.1</span></div>
    </div>
    <div class="topbar-actions">
      <button class="btn" id="exportCSV">↑ Export CSV</button>
      <button class="btn" id="installHooks">⚡ Hooks</button>
      <button class="btn" id="revealLogs">📂 Logs</button>
      <button class="btn primary" id="refresh">↻ Refresh</button>
    </div>
  </div>
  <div class="content">
    <div class="kpi-grid" id="cards"></div>
    <div class="row">
      <div class="col">
        <div class="glass accent-top">
          <div class="chart-wrap">
            <div class="section-hdr"><span class="dot"></span><h2>Daily Savings — Last 30 Days</h2><span class="line"></span></div>
            <div class="chart" id="chart"><div class="empty">No data yet</div></div>
          </div>
        </div>
      </div>
      <div class="col">
        <div class="glass accent-top">
          <div class="section-hdr" style="margin:12px 12px 0"><span class="dot"></span><h2>Weekly Comparison</h2><span class="line"></span></div>
          <div id="weekly"><div class="empty" style="padding:24px 12px">Need at least 1 week of data</div></div>
        </div>
      </div>
    </div>
    <div class="row">
      <div class="col">
        <div class="glass accent-top">
          <div class="section-hdr" style="margin:12px 12px 0"><span class="dot"></span><h2>Archetype Distribution</h2><span class="line"></span></div>
          <div id="archetype"><div class="empty" style="padding:24px 12px">No runs yet</div></div>
        </div>
      </div>
      <div class="col">
        <div class="glass accent-top">
          <div class="section-hdr" style="margin:12px 12px 0"><span class="dot"></span><h2>Top Commands</h2><span class="line"></span></div>
          <table id="topTable" style="margin:0 4px"><thead><tr><th>Command</th><th>Runs</th><th>Saved</th><th>Avg</th></tr></thead>
            <tbody><tr><td colspan="4" class="empty">No commands yet</td></tr></tbody>
          </table>
        </div>
      </div>
    </div>
    <div class="glass accent-top">
      <div class="section-hdr" style="margin:12px 12px 0"><span class="dot"></span><h2>Recent Runs</h2><span class="line"></span></div>
      <table id="recentTable" style="margin:0 4px">
        <thead><tr><th>Time</th><th>Command</th><th>Savings</th><th>Tokens</th><th>Wall</th><th>Exit</th><th>Efficiency</th></tr></thead>
        <tbody><tr><td colspan="7" class="empty">Waiting for first run…</td></tr></tbody>
      </table>
    </div>
  </div>
  <div class="footer">
    <span><span class="dot"></span> <span id="updated">Awaiting data</span></span>
    <span>Shard Metrics v0.1</span>
  </div>
</div>
<script nonce="${nonce}">
  const vscode = acquireVsCodeApi();
  const fmt = new Intl.NumberFormat('en-US');
  const cardsEl=document.getElementById('cards'),chartEl=document.getElementById('chart'),weeklyEl=document.getElementById('weekly'),archEl=document.getElementById('archetype'),topTbody=document.querySelector('#topTable tbody'),recentTbody=document.querySelector('#recentTable tbody'),updatedEl=document.getElementById('updated');
  document.getElementById('refresh').onclick=()=>vscode.postMessage({type:'refresh'});
  document.getElementById('installHooks').onclick=()=>vscode.postMessage({type:'installHooks'});
  document.getElementById('revealLogs').onclick=()=>vscode.postMessage({type:'revealLogs'});
  document.getElementById('exportCSV').onclick=()=>vscode.postMessage({type:'exportCSV'});

  // ── Efficiency badge ──
  function effBadge(pct) {
    const cls = pct >= 60 ? 'high' : pct >= 30 ? 'mid' : 'low';
    const label = pct >= 80 ? 'Superior' : pct >= 60 ? 'Strong' : pct >= 40 ? 'Solid' : pct >= 20 ? 'Moderate' : 'Low';
    return '<span class="eff-badge '+cls+'">● '+label+'</span>';
  }

  // ── KPI card ──
  function kpi(lbl, val, sub, barPct, barCls, accent) {
    const bar = barPct !== undefined
      ? '<div class="bar-track"><div class="bar-fill '+(barCls||'gold')+'" style="width:'+Math.min(100,Math.max(0,barPct))+'%"></div></div>'
      : '';
    return '<div class="kpi'+(accent?' '+accent:'')+'"><div class="lbl">'+lbl+'</div><div class="val">'+val+'</div>'+(sub?'<div class="sub">'+sub+'</div>':'')+bar+'</div>';
  }

  // ── Main render ──
  function render(snap) {
    const s=snap.summary||{},showCost=snap._showCost!==false,model=snap._model||'claude-sonnet-4',pIn=snap._priceInput||3,pOut=snap._priceOutput||15,savedTokens=s.tokens_saved||0,costSaved=(savedTokens/1000)*((pIn+pOut)/2),pctStr=(s.savings_pct||0).toFixed(1);
    let ch='';
    ch+=kpi('Runs',fmt.format(s.total_commands||0),'commands proxied',void 0,void 0,'accent-steel');
    ch+=kpi('Tokens Saved',fmt.format(savedTokens),pctStr+'% efficiency',s.savings_pct||0,'gold','accent-gold');
    ch+=kpi('Output',fmt.format(s.tokens_out||0)+' out',s.tokens_in>0?'ratio: '+(s.tokens_out/(s.tokens_in||1)*100).toFixed(1)+'%':'','','accent-sage');
    ch+=kpi('Cost Saved','$'+costSaved.toFixed(2),'at '+model+' rates',showCost?Math.min(100,costSaved*20):void 0,'gold','accent-gold');
    ch+=kpi('Avg Exec',(s.avg_exec_ms||0)+' ms',fmt.format(s.total_commands||0)+' runs',void 0,void 0,'accent-warm');
    ch+=kpi('Efficiency','',pctStr+'% savings',s.savings_pct||0,'sage','accent-sage');
    cardsEl.innerHTML=ch;

    // Daily chart
    const daily=snap.daily||[];
    if(!daily.length){chartEl.innerHTML='<div class="empty">No data yet</div>'}else{
      const max=Math.max(1,...daily.map(d=>d.tokens_in-d.tokens_out));
      chartEl.innerHTML=daily.map(d=>{
        const sv=Math.max(0,d.tokens_in-d.tokens_out);
        return '<div class="day"><div class="val">'+fmt.format(sv)+'</div><div class="bar" style="height:'+Math.max(2,Math.round((sv/max)*100))+'%"></div><div class="lbl">'+d.day.slice(5)+'</div></div>';
      }).join('');
    }

    // Weekly comparison
    if(daily.length>=2){
      const td=new Date(),tws=new Date(td);tws.setDate(td.getDate()-td.getDay());
      const lws=new Date(tws);lws.setDate(lws.getDate()-7);
      const fd=d=>d.toISOString().slice(0,10);
      let tt=0,lt=0,tr=0,lr=0;
      for(const d of daily){
        if(d.day>=fd(tws)){tt+=Math.max(0,d.tokens_in-d.tokens_out);tr+=d.runs}
        else if(d.day>=fd(lws)){lt+=Math.max(0,d.tokens_in-d.tokens_out);lr+=d.runs}
      }
      const pc=lt>0?((tt-lt)/lt*100).toFixed(1):'—',ar=lt>0?(tt>lt?'▲':'▼'):'—',cl=tt>lt?'change-up':'change-down';
      weeklyEl.innerHTML='<div class="weekly"><div class="stat"><div class="wlbl">This Week</div><div class="wval">'+fmt.format(tt)+'</div><div class="wsub">'+tr+' runs</div></div><div class="stat"><div class="wlbl">Last Week</div><div class="wval">'+fmt.format(lt)+'</div><div class="wsub">'+lr+' runs</div></div><div class="stat"><div class="wlbl">Change</div><div class="wval '+cl+'">'+(pc==='—'?'—':pc+'%')+'</div><div class="wsub">'+ar+'</div></div></div>';
    }

    // Archetype donut
    const archC=['#c9a45c','#8a9b8e','#b8957a','rgba(255,255,255,.05)'],archN=['Tabular','Linear-Log','Tree','Passthrough'],top=snap.topCommands||[],ts=top.reduce((a,t)=>a+t.saved,0)||1,archS=top.slice(0,4).map((t,i)=>({name:archN[i]||t.command, pct:(t.saved/ts*100).toFixed(0), color:archC[i]||'#888'}));
    if(archS.length){
      let gp=[],ac=0;
      for(const a of archS){const p=parseFloat(a.pct);gp.push(a.color+' '+ac+'% '+(ac+p)+'%');ac+=p}
      archEl.innerHTML='<div class="donut-wrap"><div class="donut" style="background:conic-gradient('+gp.join(',')+')"></div><ul class="donut-legend">'+archS.map(a=>'<li><span class="swatch" style="background:'+a.color+'"></span><span class="dim">'+a.pct+'%</span> '+a.name+'</li>').join('')+'</ul></div>';
    }

    // Top commands
    if(!top.length){topTbody.innerHTML='<tr><td colspan="4" class="empty">No commands yet</td></tr>'}else{
      topTbody.innerHTML=top.map(t=>'<tr><td>'+esc(t.command)+'</td><td>'+t.runs+'</td><td class="gold">'+fmt.format(t.saved)+'</td><td class="'+(t.pct<20?'muted':'gold')+'">'+(t.pct.toFixed(1))+'%</td></tr>').join('');
    }

    // Recent runs
    const recent=snap.recent||[];
    if(!recent.length){recentTbody.innerHTML='<tr><td colspan="7" class="empty">Waiting for first run…</td></tr>'}else{
      recentTbody.innerHTML=recent.map(r=>{
        const cmd=(r.argv||[]).slice(0,4).join(' '),pct=(r.savings_pct||0).toFixed(1);
        return '<tr data-log="'+esc(r.log_path||'')+'"><td class="dim">'+r.started_at.replace('T',' ').slice(5,16)+'</td><td>'+esc(cmd)+'</td><td class="'+(r.savings_pct>=60?'gold':r.savings_pct>=20?'warm':'')+'">'+pct+'%</td><td class="dim">'+fmt.format(r.tokens_in)+' → <span class="gold">'+fmt.format(r.tokens_out)+'</span></td><td class="dim">'+r.wall_ms+'ms</td><td class="'+(r.exit_code===0?'sage':'warm')+'">'+(r.exit_code===0?'OK':'ERR:'+r.exit_code)+'</td><td>'+effBadge(r.savings_pct||0)+'</td></tr>'
      }).join('');
      recentTbody.querySelectorAll('tr').forEach(tr=>{tr.addEventListener('click',()=>{const p=tr.getAttribute('data-log');if(p)vscode.postMessage({type:'openLog',path:p})})});
    }

    updatedEl.textContent='Online · Last updated: '+new Date().toLocaleTimeString();
  }

  function esc(s){return String(s).replace(/[&<>"']/g,c=>({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}[c]))}
  window.addEventListener('message',e=>{const m=e.data;if(m?.type==='update'&&m.snapshot)render(m.snapshot)});
  vscode.postMessage({type:'ready'});
</script>
</body>
</html>`;
}

function mkNonce(): string {
  let s = '';
  const chars = 'abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789';
  for (let i = 0; i < 24; i++) s += chars.charAt(Math.random() * chars.length);
  return s;
}
