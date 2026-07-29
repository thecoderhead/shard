// Seed demo data using sql.js (pure WASM, no native deps).
const initSqlJs = require('sql.js');
const fs = require('fs');
const path = require('path');

async function main() {
  const SQL = await initSqlJs();
  const dbDir = path.join(__dirname, '..', '.shard');
  const logsDir = path.join(dbDir, 'logs');
  fs.mkdirSync(logsDir, { recursive: true });

  const dbPath = path.join(dbDir, 'metrics.db');
  const db = new SQL.Database();
  db.run(`CREATE TABLE IF NOT EXISTS runs (
    id TEXT PRIMARY KEY, argv TEXT NOT NULL, cwd TEXT, started_at TEXT NOT NULL,
    finished_at TEXT, wall_ms INTEGER NOT NULL, exit_code INTEGER DEFAULT 0,
    raw_bytes INTEGER DEFAULT 0, tokens_in INTEGER NOT NULL, tokens_out INTEGER NOT NULL,
    savings_pct REAL NOT NULL, log_path TEXT, intent TEXT, is_tty INTEGER DEFAULT 0
  )`);

  const cmds = [
    { cmd: ['git','status'], in: 1240, out: 110 },
    { cmd: ['cargo','test'], in: 28400, out: 2100 },
    { cmd: ['docker','ps','-a'], in: 3200, out: 280 },
    { cmd: ['npm','install'], in: 8200, out: 650 },
    { cmd: ['kubectl','get','pods','--all-namespaces'], in: 15000, out: 320 },
    { cmd: ['cargo','build'], in: 42000, out: 3800 },
    { cmd: ['git','diff','HEAD~1'], in: 2800, out: 450 },
    { cmd: ['npm','test'], in: 18500, out: 1400 },
    { cmd: ['npx','eslint','src/'], in: 6400, out: 520 },
    { cmd: ['cargo','clippy'], in: 12000, out: 890 },
    { cmd: ['docker','compose','up','-d'], in: 980, out: 85 },
    { cmd: ['git','log','--oneline','-20'], in: 760, out: 95 },
    { cmd: ['ps','aux'], in: 3100, out: 220 },
    { cmd: ['ls','-la'], in: 420, out: 65 },
    { cmd: ['tree','src/'], in: 2100, out: 180 },
    { cmd: ['pnpm','install'], in: 7200, out: 580 },
    { cmd: ['cargo','test','--','test_auth'], in: 3200, out: 410 },
    { cmd: ['grep','-r','TODO','src/'], in: 560, out: 75 },
    { cmd: ['terraform','plan'], in: 9800, out: 720 },
    { cmd: ['cargo','fmt','--check'], in: 340, out: 52 },
    { cmd: ['git','pull'], in: 880, out: 95 },
    { cmd: ['pip','install','-r','requirements.txt'], in: 6500, out: 490 },
    { cmd: ['ruff','check','src/'], in: 4800, out: 380 },
    { cmd: ['yarn','build'], in: 38000, out: 2900 },
    { cmd: ['mvn','test'], in: 52000, out: 4100 },
  ];

  const stmt = db.prepare(`INSERT OR IGNORE INTO runs
    (id, argv, cwd, started_at, finished_at, wall_ms, exit_code, raw_bytes,
     tokens_in, tokens_out, savings_pct, log_path, is_tty)
    VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)`);

  const now = Date.now();
  let count = 0;

  for (let i = 0; i < cmds.length; i++) {
    const c = cmds[i];
    for (let j = 0; j < 3 + (i % 4); j++) {
      const dayOffset = i * 0.7 + j * 0.2;
      const ts = new Date(now - dayOffset * 86400000 - j * 3600000);
      const id = `demo-${String(i).padStart(2, '0')}-${j}`;
      const wall = 200 + Math.floor(Math.random() * 3000);
      const tokensIn = c.in + Math.floor(Math.random() * 500);
      const tokensOut = Math.max(10, Math.floor(c.out * (0.85 + Math.random() * 0.3)));
      const pct = ((tokensIn - tokensOut) / tokensIn * 100);
      stmt.run([id, JSON.stringify(c.cmd), dbDir, ts.toISOString(), ts.toISOString(),
        wall, 0, tokensIn * 4, tokensIn, tokensOut, parseFloat(pct.toFixed(1)), path.join(logsDir, `${id}.log`), 0]);
      count++;
    }
  }
  stmt.free();

  // Write WAL-style DB (sql.js can export binary)
  fs.writeFileSync(dbPath, Buffer.from(db.export()));
  console.log(`Seeded ${count} demo runs into ${dbPath}`);
  db.close();
}

main().catch(e => { console.error(e); process.exit(1); });
