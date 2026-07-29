import * as vscode from 'vscode';
import { MetricsReader, MetricsSnapshot } from './metrics';

export class SummaryViewProvider implements vscode.WebviewViewProvider {
  public static readonly viewType = 'shard.summary';
  private _view?: vscode.WebviewView;

  constructor(
    private readonly _extensionUri: vscode.Uri,
    private readonly reader: MetricsReader,
  ) {}

  resolveWebviewView(
    webviewView: vscode.WebviewView,
    _ctx: vscode.WebviewViewResolveContext,
    _token: vscode.CancellationToken,
  ) {
    this._view = webviewView;
    webviewView.webview.options = {
      enableScripts: true,
      localResourceRoots: [this._extensionUri],
    };
    webviewView.webview.html = this._html();

    webviewView.webview.onDidReceiveMessage(async msg => {
      if (msg?.type === 'command' && typeof msg.id === 'string') {
        vscode.commands.executeCommand(msg.id);
      } else if (msg?.type === 'ready') {
        this.pushUpdate(await this.reader.snapshot());
      }
    });
  }

  pushUpdate(snapshot: MetricsSnapshot) {
    this._view?.webview.postMessage({ type: 'update', snapshot });
  }

  private _html(): string {
    const nonce = mkNonce();
    return /* html */`<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="UTF-8"/>
<meta name="viewport" content="width=device-width,initial-scale=1.0"/>
<meta http-equiv="Content-Security-Policy"
      content="default-src 'none'; style-src 'unsafe-inline'; script-src 'nonce-${nonce}';">
<style>
  :root {
    --bg-deep:     #111112;
    --bg-glass:    rgba(30,30,32,0.6);
    --glass-border:     rgba(255,255,255,.05);
    --glass-border-hi:  rgba(255,255,255,.09);
    --glass-shine:      rgba(255,255,255,.025);
    --accent-gold:    #c9a45c;
    --accent-sage:    #8a9b8e;
    --accent-warm:    #b8957a;
    --accent-steel:   #8899aa;
    --grad-gold: linear-gradient(135deg,#c9a45c,#b8957a);
    --glow-gold:   0 0 24px rgba(201,164,92,.06);
    --glow-subtle: 0 0 8px rgba(255,255,255,.015);
    --text-primary:   #e8e6e1;
    --text-secondary: #b0aba4;
    --text-tertiary:  #7a7670;
    --font-display: system-ui,-apple-system,'Segoe UI',Roboto,Helvetica,Arial,sans-serif;
    --font-mono:  var(--vscode-editor-font-family,'JetBrains Mono','Cascadia Code','Fira Code','Consolas',monospace);
    --radius-md:  8px;
    --radius-lg:  12px;
  }
  *{box-sizing:border-box;margin:0;padding:0}
  body{
    font-family:var(--font-display);font-size:var(--vscode-font-size,11px);
    color:var(--text-primary);padding:8px;background:transparent;
  }
  .glass{
    background:var(--bg-glass);backdrop-filter:blur(16px) saturate(1.5);
    -webkit-backdrop-filter:blur(16px) saturate(1.5);
    border:1px solid var(--glass-border);border-radius:var(--radius-lg);
    box-shadow:var(--glow-subtle),inset 0 1px 0 var(--glass-shine);
    position:relative;overflow:hidden;transition:border-color .35s;
  }
  .glass::before{
    content:'';position:absolute;top:0;left:0;right:0;height:1px;
    background:linear-gradient(135deg,rgba(255,255,255,.06) 0%,transparent 50%);
    pointer-events:none;
  }
  .hero{padding:12px;margin-bottom:8px}
  .hero-lbl{font-size:9px;font-weight:500;text-transform:uppercase;letter-spacing:.08em;color:var(--text-tertiary);margin-bottom:4px}
  .hero-val{font-size:22px;font-weight:600;color:var(--accent-gold);font-family:var(--font-mono);line-height:1.15;font-variant-numeric:tabular-nums}
  .prog{height:2px;background:rgba(255,255,255,.04);margin:8px 0 6px;overflow:hidden;border-radius:99px}
  .prog-fill{height:100%;border-radius:99px;background:var(--grad-gold);transition:width .6s ease}
  .hero-sub{font-size:10px;color:var(--text-tertiary);font-weight:400}
  .grid{display:grid;grid-template-columns:1fr 1fr;gap:5px;margin-bottom:8px}
  .stat{
    padding:8px;background:var(--bg-glass);backdrop-filter:blur(16px) saturate(1.5);
    -webkit-backdrop-filter:blur(16px) saturate(1.5);
    border:1px solid var(--glass-border);border-radius:var(--radius-md);transition:border-color .35s;
  }
  .stat:hover{border-color:var(--glass-border-hi)}
  .stat-val{font-size:14px;font-weight:600;font-family:var(--font-mono);color:var(--text-primary);font-variant-numeric:tabular-nums}
  .stat-lbl{font-size:8px;color:var(--text-tertiary);margin-top:2px;text-transform:uppercase;letter-spacing:.08em;font-weight:500}
  .actions{display:flex;flex-direction:column;gap:4px;margin-bottom:6px}
  .btn{
    width:100%;padding:5px 10px;border:1px solid var(--glass-border);border-radius:99px;cursor:pointer;
    font-family:var(--font-display);font-size:10px;font-weight:500;background:transparent;color:var(--text-secondary);
    letter-spacing:.02em;text-align:left;transition:all .2s;display:flex;align-items:center;gap:5px;
  }
  .btn:hover{background:rgba(255,255,255,.04);border-color:var(--glass-border-hi);color:var(--text-primary)}
  .btn.primary{background:rgba(201,164,92,.08);border-color:rgba(201,164,92,.2);color:var(--accent-gold)}
  .btn.primary:hover{background:rgba(201,164,92,.12);border-color:rgba(201,164,92,.3)}
  .footer{font-size:9px;color:var(--text-tertiary);padding-top:6px;border-top:1px solid rgba(255,255,255,.04);letter-spacing:.02em;margin-top:4px;display:flex;align-items:center;gap:4px}
  .footer .dot{display:inline-block;width:4px;height:4px;border-radius:50%;background:var(--accent-sage)}
  .empty{color:var(--text-tertiary);text-align:center;line-height:1.6;font-size:11px;padding:20px 8px}
  .empty .icon{display:block;margin:0 auto 8px;width:32px;height:32px;border-radius:var(--radius-md);background:var(--grad-gold);color:#111;font-size:14px;font-weight:700;line-height:32px;text-align:center;font-family:var(--font-mono)}
  @keyframes fadeSlideIn{from{opacity:0;transform:translateY(4px)}to{opacity:1;transform:translateY(0)}}
</style>
</head>
<body>
<div id="empty" class="empty glass">
  <div class="icon">◆</div>
  No data yet.<br>
  Install shell hooks to start capturing.
</div>
<div id="stats" style="display:none">
  <div class="hero glass">
    <div class="hero-lbl">Tokens Saved</div>
    <div class="hero-val" id="saved">—</div>
    <div class="prog"><div class="prog-fill" id="bar" style="width:0%"></div></div>
    <div class="hero-sub" id="pct-sub">— · — runs</div>
  </div>
  <div class="grid">
    <div class="stat"><div class="stat-val gold" id="pct">—</div><div class="stat-lbl">Savings</div></div>
    <div class="stat"><div class="stat-val" id="runs">—</div><div class="stat-lbl">Runs</div></div>
    <div class="stat"><div class="stat-val" id="out">—</div><div class="stat-lbl">Out Tokens</div></div>
    <div class="stat"><div class="stat-val" id="avg">—</div><div class="stat-lbl">Avg Exec</div></div>
  </div>
</div>
<div class="actions">
  <button class="btn primary" id="bDash">⬡ Dashboard</button>
  <button class="btn" id="bRefresh">↻ Refresh</button>
  <button class="btn" id="bHooks">⚡ Hooks</button>
  <button class="btn" id="bLogs">📂 Logs</button>
</div>
<div class="footer"><span class="dot"></span> <span id="footer">Offline · —</span></div>
<script nonce="${nonce}">
  const vscode = acquireVsCodeApi();
  const fmt = new Intl.NumberFormat('en-US');
  document.getElementById('bDash').onclick = () => vscode.postMessage({type:'command',id:'shard.showDashboard'});
  document.getElementById('bRefresh').onclick = () => vscode.postMessage({type:'command',id:'shard.refreshMetrics'});
  document.getElementById('bHooks').onclick = () => vscode.postMessage({type:'command',id:'shard.installHooks'});
  document.getElementById('bLogs').onclick = () => vscode.postMessage({type:'command',id:'shard.revealLogs'});
  window.addEventListener('message', e => { if (e.data?.type === 'update') render(e.data.snapshot); });
  function render(snap) {
    const s = snap?.summary ?? {};
    const has = (s.total_commands ?? 0) > 0;
    document.getElementById('empty').style.display = has ? 'none' : '';
    document.getElementById('stats').style.display = has ? '' : 'none';
    if (has) {
      const pct = (s.savings_pct ?? 0).toFixed(1);
      document.getElementById('saved').textContent = fmt.format(s.tokens_saved ?? 0);
      document.getElementById('bar').style.width = Math.min(100, s.savings_pct ?? 0) + '%';
      document.getElementById('pct-sub').textContent = pct + '% · ' + fmt.format(s.total_commands) + ' runs';
      document.getElementById('pct').textContent = pct + '%';
      document.getElementById('runs').textContent = fmt.format(s.total_commands ?? 0);
      document.getElementById('out').textContent = fmt.format(s.tokens_out ?? 0);
      document.getElementById('avg').textContent = (s.avg_exec_ms ?? 0) + 'ms';
    }
    document.getElementById('footer').textContent = 'Online · ' + new Date().toLocaleTimeString();
  }
  vscode.postMessage({type:'ready'});
</script>
</body>
</html>`;
  }
}

function mkNonce(): string {
  let s = '';
  const chars = 'ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789';
  for (let i = 0; i < 24; i++) s += chars[Math.floor(Math.random() * chars.length)];
  return s;
}
