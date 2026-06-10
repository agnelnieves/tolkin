#!/usr/bin/env node
"use strict";

const { spawnSync } = require("node:child_process");
const fs = require("node:fs");
const path = require("node:path");

const PLATFORMS = {
  "darwin-arm64": "tolkin-darwin-arm64",
  "darwin-x64": "tolkin-darwin-x64",
  "linux-x64": "tolkin-linux-x64",
  "linux-arm64": "tolkin-linux-arm64",
  "win32-x64": "tolkin-win32-x64",
};

const key = `${process.platform}-${process.arch}`;
const pkg = PLATFORMS[key];
// Windows binaries carry the .exe suffix; all other platforms have no extension.
const binName = process.platform === "win32" ? "tolkin.exe" : "tolkin";

function fail() {
  console.error(`tolkin: unsupported platform ${key}.`);
  console.error("Supported today: macOS arm64 (Apple Silicon), macOS x64 (Intel), Linux x64, Linux arm64, and Windows x64.");
  process.exit(1);
}

if (!pkg) fail();

let binPath = null;

// Development mode: sibling platform package in the source tree.
const devPath = path.join(__dirname, "..", "..", pkg, "bin", binName);
if (fs.existsSync(devPath)) {
  binPath = devPath;
} else {
  try {
    binPath = require.resolve(`${pkg}/bin/${binName}`);
  } catch {
    console.error(`tolkin: could not find the ${pkg} package.`);
    console.error("It ships as an optionalDependency of tolkin; your installer may have skipped it.");
    console.error("Try reinstalling: npm install tolkin (or bun add tolkin).");
    console.error("Supported today: macOS arm64, macOS x64, Linux x64, Linux arm64, and Windows x64.");
    process.exit(1);
  }
}

const result = spawnSync(binPath, process.argv.slice(2), { stdio: "inherit" });

if (result.error) {
  console.error(`tolkin: failed to launch binary: ${result.error.message}`);
  process.exit(1);
}
if (result.signal) {
  process.kill(process.pid, result.signal);
}
process.exit(result.status === null ? 1 : result.status);
