#!/usr/bin/env node

// shard — resolve the native binary installed alongside this package.
// The Rust release binary is placed alongside this file during install.

const { resolve } = require("path");
const { spawn } = require("child_process");

const binName = process.platform === "win32" ? "shard.exe" : "shard";
const binPath = resolve(__dirname, binName);

const child = spawn(binPath, process.argv.slice(2), {
  stdio: "inherit",
});

child.on("exit", (code) => {
  process.exit(code ?? 1);
});
