#!/usr/bin/env node
const { existsSync, copyFileSync, chmodSync, unlinkSync, readdirSync, statSync } = require("fs");
const { resolve, join } = require("path");
const { execSync, spawnSync } = require("child_process");

// Recursively find the native binary under dir (archives may nest it).
function findBinary(dir) {
  for (const name of readdirSync(dir)) {
    const p = join(dir, name);
    if (name === BIN_NAME) return p;
    if (statSync(p).isDirectory()) {
      const found = findBinary(p);
      if (found) return found;
    }
  }
  return null;
}

const pkg = require(resolve(__dirname, "..", "package.json"));
const BIN_NAME = process.platform === "win32" ? "shard.exe" : "shard";
const BIN_DIR = resolve(__dirname);
const TARGET_BIN = resolve(BIN_DIR, BIN_NAME);
const REPO = (pkg.repository?.url || "").replace(/https:\/\/github\.com\//, "").replace(/\.git$/, "") || "thecoderhead/shard";
// Use a fixed release tag (separate from npm package version).
// Bump this when a new GitHub Release with binaries is published.
const VERSION = "v0.1.0";

const TARGET_MAP = {
  "win32-x64":      ["x86_64-pc-windows-msvc",  ".zip"],
  "win32-arm64":    ["aarch64-pc-windows-msvc", ".zip"],
  "darwin-x64":     ["x86_64-apple-darwin",      ".tar.gz"],
  "darwin-arm64":   ["aarch64-apple-darwin",     ".tar.gz"],
  "linux-x64":      ["x86_64-unknown-linux-gnu", ".tar.gz"],
  "linux-arm64":    ["aarch64-unknown-linux-gnu",".tar.gz"],
};

if (existsSync(TARGET_BIN)) process.exit(0);

// Check for existing build in source tree
for (const dir of ["release", "debug"]) {
  const p = resolve(__dirname, "..", "target", dir, BIN_NAME);
  if (existsSync(p)) {
    copyFileSync(p, TARGET_BIN);
    if (process.platform !== "win32") chmodSync(TARGET_BIN, 0o755);
    process.exit(0);
  }
}

// Download pre-built binary from GitHub Releases
const key = process.platform + "-" + process.arch;
const entry = TARGET_MAP[key];

if (entry) {
  const [target, ext] = entry;
  const archiveName = ext === ".zip" ? `shard-${target}.exe.zip` : `shard-${target}${ext}`;
  const url = `https://github.com/${REPO}/releases/download/${VERSION}/${archiveName}`;
  const tmp = resolve(__dirname, ".shard-dl-" + process.pid + (ext === ".zip" ? ".zip" : ""));

  console.log("shard: downloading pre-built binary...");
  try {
    // Try curl (available on Windows 10+), fall back to PowerShell
    try {
      execSync(`curl.exe -sfL "${url}" -o "${tmp}"`, { stdio: "pipe", timeout: 120000 });
    } catch {
      execSync(`powershell -command "[Net.ServicePointManager]::SecurityProtocol = 'tls12, tls11, tls'; \$wc = New-Object Net.WebClient; \$wc.Headers.Add('User-Agent', 'shard-installer/1.0'); \$wc.DownloadFile('${url}', '${tmp}')"`, { stdio: "pipe", timeout: 120000 });
    }

    if (ext === ".zip") {
      // Try PowerShell Expand-Archive first, then 7z
      const r = spawnSync("powershell", ["-command", `Expand-Archive -Path '${tmp}' -DestinationPath '${BIN_DIR}' -Force`]);
      if (r.status !== 0) {
        spawnSync("7z", ["x", tmp, "-y", "-o" + BIN_DIR], { stdio: "pipe" });
      }
    } else {
      // tar.gz — extract to BIN_DIR, binary name inside is just "shard"
      execSync(`tar xzf "${tmp}" -C "${BIN_DIR}"`, { stdio: "pipe" });
    }
    // The archive may nest the binary (e.g. target/<triple>/release/shard.exe),
    // so locate it wherever it landed and move it into place.
    if (!existsSync(TARGET_BIN)) {
      const found = findBinary(BIN_DIR);
      if (found) {
        copyFileSync(found, TARGET_BIN);
      }
    }
    if (process.platform !== "win32") chmodSync(TARGET_BIN, 0o755);
    if (!existsSync(TARGET_BIN)) {
      throw new Error("extracted archive did not contain " + BIN_NAME);
    }
    unlinkSync(tmp);
    console.log("shard: installed native binary at " + TARGET_BIN);
    process.exit(0);
  } catch (e) {
    try { unlinkSync(tmp); } catch {}
    console.log("shard: download failed, building from source...");
  }
}

// Fallback: build from source
console.log("shard: building from source (this may take a moment)...");
try {
  execSync("cargo build --release", { cwd: resolve(__dirname, ".."), stdio: "inherit" });
  copyFileSync(resolve(__dirname, "..", "target", "release", BIN_NAME), TARGET_BIN);
  if (process.platform !== "win32") chmodSync(TARGET_BIN, 0o755);
  console.log("shard: built and installed native binary at " + TARGET_BIN);
} catch {
  console.error(
    "shard: could not build from source. Install Rust:\n" +
    "  winget install --id Rustlang.Rustup -e       (Windows)\n" +
    "  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh   (macOS/Linux)\n"
  );
  process.exit(1);
}
