'use strict';
const { test } = require('node:test');
const assert = require('node:assert/strict');
const { EventEmitter } = require('node:events');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const { platformSuffix, resolveBinary, launch } = require('../bin/launcher.cjs');

test('supports only shipped targets and rejects musl', () => {
  for (const [platform, arch] of [['linux', 'x64'], ['win32', 'x64'], ['darwin', 'x64'], ['darwin', 'arm64']]) {
    assert.equal(platformSuffix(platform, arch, true), `${platform}-${arch}`);
  }
  assert.throws(() => platformSuffix('linux', 'x64', false), /glibc/);
  assert.throws(() => platformSuffix('linux', 'arm64', true), /Unsupported/);
});

test('resolves an exact-version optional package with actionable missing errors', (t) => {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'mcp-launcher-'));
  t.after(() => fs.rmSync(dir, { recursive: true, force: true }));
  const manifest = { name: '@jys/opensource-maintainer-mcp', version: '0.1.0' };
  const file = path.join(dir, 'package.json');
  fs.writeFileSync(file, JSON.stringify({ name: `${manifest.name}-win32-x64`, version: '0.1.0' }));
  assert.equal(resolveBinary(manifest, 'win32-x64', () => file), path.join(dir, 'bin', 'opensource-maintainer-mcp.exe'));
  assert.throws(() => resolveBinary(manifest, 'linux-x64', () => { throw new Error('missing'); }), /include=optional/);
  fs.writeFileSync(file, JSON.stringify({ name: `${manifest.name}-win32-x64`, version: '0.2.0' }));
  assert.throws(() => resolveBinary(manifest, 'win32-x64', () => file), /version/);
  fs.writeFileSync(file, 'not JSON with SECRET data');
  assert.throws(() => resolveBinary(manifest, 'win32-x64', () => file), /Unable to read native package metadata/);
});

function fixture() {
  const proc = new EventEmitter();
  proc.platform = 'linux';
  proc.pid = 42;
  proc.stderr = { write: (text) => { proc.diagnostic = text; } };
  proc.kill = (...args) => { proc.killed = args; };
  const child = new EventEmitter();
  child.kill = (signal) => { child.forwarded = signal; };
  const start = (...args) => { child.invocation = args; return child; };
  return { proc, child, start };
}

test('forwards literal arguments, stdio, signals, and exit codes', () => {
  const { proc, child, start } = fixture();
  const args = ['config with spaces.toml', '$(never executed)'];
  launch('/binary with spaces', args, proc, start);
  assert.deepEqual(child.invocation, ['/binary with spaces', args, { stdio: 'inherit', windowsHide: true }]);
  proc.emit('SIGTERM');
  assert.equal(child.forwarded, 'SIGTERM');
  child.emit('close', 7, null);
  assert.equal(proc.exitCode, 7);
  assert.equal(proc.listenerCount('SIGTERM'), 0);
  assert.equal(proc.diagnostic, undefined);
});

test('preserves POSIX signal termination and reports startup failures on stderr', () => {
  let { proc, child, start } = fixture();
  launch('/binary', [], proc, start);
  child.emit('close', null, 'SIGTERM');
  assert.deepEqual(proc.killed, [42, 'SIGTERM']);
  ({ proc, child, start } = fixture());
  launch('/binary', [], proc, start);
  child.emit('error', new Error('untrusted details'));
  child.emit('close', -2, null);
  assert.equal(proc.exitCode, 1);
  assert.match(proc.diagnostic, /Unable to start/);
  assert.doesNotMatch(proc.diagnostic, /untrusted/);
});
