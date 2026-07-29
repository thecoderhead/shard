#!/usr/bin/env node

// postinstall script — downloads or copies the pre-built shard binary
// for the current platform to `bin/shard` (or `bin/shard.exe` on Windows).
//
// Currently this package requires a pre-existing Rust toolchain to build
// shard from source. Once pre-built binaries are published on GitHub
// Releases this script will download the correct artifact for the
// detected platform automatically.

const { existsSync, copyFileSync, chmodSync } = require("fs");
const { resolve } = require("path");
const { execSync } = require("child_process");

const BIN_NAME = process.platform === "win32" ? "shard.exe" : "shard";
const BIN_DIR = resolve(__dirname);
const TARGET_BIN = resolve(BIN_DIR, BIN_NAME);

// 1. Check if a binary already exists next to this script.
if (existsSync(TARGET_BIN)) {
  console.log(`shard: native binary found at ${TARGET_BIN}`);
  process.exit(0);
}

// 2. Check for a release build in the project source tree.
const RELEASE_BIN = resolve(__dirname, "..", "target", "release", BIN_NAME);
const DEBUG_BIN = resolve(__dirname, "..", "target", "debug", BIN_NAME);

for (const candidate of [RELEASE_BIN, DEBUG_BIN]) {
  if (existsSync(candidate)) {
    console.log(`shard: copying native binary from ${candidate}`);
    copyFileSync(candidate, TARGET_BIN);
    if (process.platform !== "win32") {
      chmodSync(TARGET_BIN, 0o755);
    }
    process.exit(0);
  }
}

// 3. Fallback: try building from source.
console.log("shard: no pre-built binary found — building from source (this may take a moment)...");
try {
  execSync("cargo build --release", {
    cwd: resolve(__dirname, ".."),
    stdio: "inherit",
  });
  copyFileSync(RELEASE_BIN, TARGET_BIN);
  if (process.platform !== "win32") {
    chmodSync(TARGET_BIN, 0o755);
  }
  console.log(`shard: built and installed native binary at ${TARGET_BIN}`);
} catch {
  console.error(
    "shard: could not build from source. Ensure the Rust toolchain is installed:\n" +
      "  winget install --id Rustlang.Rustup -e       (Windows)\n" +
      "  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh   (macOS/Linux)\n"
  );
  process.exit(1);
}
