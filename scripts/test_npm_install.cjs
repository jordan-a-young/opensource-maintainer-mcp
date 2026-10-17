'use strict';
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const os = require('node:os');
const { spawn } = require('node:child_process');
const { npm, pack, output } = require('./npm_packages.cjs');
const { platformSuffix } = require('../npm/bin/launcher.cjs');
const root = path.resolve(__dirname, '..');
const suffix = platformSuffix(process.platform, process.arch,
  process.platform !== 'linux' || process.report.getReport().header.glibcVersionRuntime);

async function smoke(directory, config) {
  // Exercise npm exec's real installed command shim, not a source-tree import.
  const child = spawn(process.execPath, [process.env.npm_execpath, 'exec', '--offline', '--no', '--',
    'opensource-maintainer-mcp', config], {
    cwd: directory, stdio: ['pipe', 'pipe', 'pipe'],
    env: { ...process.env, npm_config_cache: path.join(root, 'npm-dist', 'cache') },
  });
  let buffer = '';
  let initialized = false;
  let tools = false;
  const send = (message) => child.stdin.write(JSON.stringify({ jsonrpc: '2.0', ...message }) + '\n');
  return new Promise((resolve, reject) => {
    const timer = setTimeout(() => { child.stdin.end(); child.kill(); reject(new Error('Installed launcher MCP test timed out')); }, 30000);
    const fail = (error) => { clearTimeout(timer); child.stdin.end(); child.kill(); reject(error); };
    child.on('error', fail);
    child.stdout.on('data', chunk => {
      buffer += chunk;
      while (buffer.includes('\n')) {
        const index = buffer.indexOf('\n');
        const line = buffer.slice(0, index);
        buffer = buffer.slice(index + 1);
        try {
          const message = JSON.parse(line);
          assert.equal(message.jsonrpc, '2.0', 'Unexpected stdout noise');
          if (message.id === 1) {
            assert(message.result);
            initialized = true;
            send({ method: 'notifications/initialized' });
            send({ id: 2, method: 'tools/list', params: {} });
          } else if (message.id === 2) {
            assert.deepEqual(message.result.tools.map(t => t.name).sort(), [
              'analyze_ci', 'audit_repository', 'compare_standards', 'get_project_overview', 'list_projects', 'review_release_config']);
            tools = true;
            child.stdin.end();
          }
        } catch (error) { fail(error); }
      }
    });
    child.stderr.resume();
    child.on('close', code => {
      clearTimeout(timer);
      try {
        assert(initialized && tools, 'MCP handshake/tool discovery failed');
        assert.equal(code, 0);
        resolve();
      } catch (error) { reject(error); }
    });
    send({ id: 1, method: 'initialize', params: { protocolVersion: '2024-11-05', capabilities: {}, clientInfo: { name: 'npm-install-test', version: '1' } } });
  });
}

async function main() {
  const packages = path.join(root, 'npm-dist', 'packages');
  const native = pack(path.join(packages, suffix));
  const launcher = pack(path.join(packages, 'launcher'));
  const temporary = fs.mkdtempSync(path.join(os.tmpdir(), 'npm mcp install '));
  try {
    fs.writeFileSync(path.join(temporary, 'package.json'), JSON.stringify({ private: true }));
    npm(['install', path.join(output, native.filename), path.join(output, launcher.filename), '--offline', '--ignore-scripts',
      '--no-audit', '--no-fund', '--package-lock=false', '--registry=http://127.0.0.1:9'], temporary);
    const config = path.join(temporary, 'config with spaces.toml');
    fs.copyFileSync(path.join(root, 'examples/config.toml'), config);
    await smoke(temporary, config);
    console.log('Offline npm tarball install, command shim, MCP handshake, six tools, and clean shutdown passed.');
  } finally {
    fs.rmSync(temporary, { recursive: true, force: true });
  }
}
main().catch(error => { console.error(error); process.exitCode = 1; });
