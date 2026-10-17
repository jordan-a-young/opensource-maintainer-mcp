#!/usr/bin/env node
'use strict';
const { spawn } = require('node:child_process');
const fs = require('node:fs');
const path = require('node:path');
const os = require('node:os');

function platformSuffix(platform, arch, glibc) {
  const suffix = `${platform}-${arch}`;
  if (!['linux-x64', 'win32-x64', 'darwin-x64', 'darwin-arm64'].includes(suffix)) {
    throw new Error(`Unsupported platform: ${suffix}. See the package's supported platforms.`);
  }
  if (platform === 'linux' && !glibc) {
    throw new Error('This Linux binary requires glibc; musl/Alpine is not supported.');
  }
  return suffix;
}

function resolveBinary(manifest, suffix, resolve = require.resolve) {
  const name = `${manifest.name}-${suffix}`;
  let packagePath;
  try {
    packagePath = resolve(`${name}/package.json`);
  } catch {
    throw new Error(`Missing native package ${name}. Reinstall with optional dependencies enabled (--include=optional).`);
  }
  let native;
  try {
    native = JSON.parse(fs.readFileSync(packagePath, 'utf8'));
  } catch {
    throw new Error('Unable to read native package metadata; reinstall the package.');
  }
  if (native.name !== name || native.version !== manifest.version) {
    throw new Error('Native package version does not match the launcher; reinstall the package.');
  }
  const executable = suffix === 'win32-x64' ? 'opensource-maintainer-mcp.exe' : 'opensource-maintainer-mcp';
  return path.join(path.dirname(packagePath), 'bin', executable);
}

function launch(binary, args, proc = process, start = spawn) {
  // No shell, downloads, compilation, or stdout diagnostics in the launcher.
  const child = start(binary, args, { stdio: 'inherit', windowsHide: true });
  const handlers = new Map();
  for (const signal of ['SIGINT', 'SIGTERM', 'SIGHUP']) {
    const forward = () => child.kill(signal);
    handlers.set(signal, forward);
    proc.on(signal, forward);
  }
  let failed = false;
  child.once('error', () => {
    failed = true;
    proc.stderr.write('Unable to start the native MCP executable; check installation and platform support.\n');
    proc.exitCode = 1;
  });
  child.once('close', (code, signal) => {
    for (const [name, handler] of handlers) proc.removeListener(name, handler);
    if (failed) return;
    if (signal) {
      proc.exitCode = 128 + (os.constants.signals[signal] || 1);
      if (proc.platform !== 'win32') proc.kill(proc.pid, signal);
    } else {
      proc.exitCode = code ?? 1;
    }
  });
  return child;
}

function main() {
  try {
    const manifest = require('../package.json');
    const glibc = process.platform !== 'linux' || process.report.getReport().header.glibcVersionRuntime;
    const suffix = platformSuffix(process.platform, process.arch, glibc);
    launch(resolveBinary(manifest, suffix), process.argv.slice(2));
  } catch (error) {
    process.stderr.write(`${error.message}\n`);
    process.exitCode = 1;
  }
}

module.exports = { platformSuffix, resolveBinary, launch };
if (require.main === module) main();
