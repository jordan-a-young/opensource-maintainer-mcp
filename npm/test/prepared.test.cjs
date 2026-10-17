'use strict';
const { test } = require('node:test');
const assert = require('node:assert/strict');
const crypto = require('node:crypto');
const fs = require('node:fs');
const path = require('node:path');
const os = require('node:os');
const { spawnSync } = require('node:child_process');
const { verifyPrepared } = require('../../scripts/npm_packages.cjs');
const source = require('../package.json');

test('requires a complete ordered and intact package set before staging', (t) => {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'npm-prepared-'));
  t.after(() => fs.rmSync(dir, { recursive: true, force: true }));
  const names = ['linux-x64', 'win32-x64', 'darwin-arm64', 'darwin-x64'].map(s => `${source.name}-${s}`).concat(source.name);
  const records = names.map((name, i) => {
    const filename = `${i}.tgz`;
    const bytes = Buffer.from(`fixture ${i}`);
    fs.writeFileSync(path.join(dir, filename), bytes);
    return { name, version: source.version, filename, sha256: crypto.createHash('sha256').update(bytes).digest('hex') };
  });
  const manifest = path.join(dir, 'packages.json');
  fs.writeFileSync(manifest, JSON.stringify(records));
  assert.equal(verifyPrepared(dir).length, 5);
  fs.writeFileSync(manifest, JSON.stringify(records.toReversed()));
  assert.throws(() => verifyPrepared(dir), /native packages before the launcher/);
  fs.writeFileSync(manifest, JSON.stringify(records));
  fs.writeFileSync(path.join(dir, '0.tgz'), 'tampered');
  assert.throws(() => verifyPrepared(dir));
});

test('staging refuses execution outside an authorized GitHub job before invoking npm', () => {
  const result = spawnSync(process.execPath, [path.resolve(__dirname, '../../scripts/npm_packages.cjs'), 'stage'], {
    encoding: 'utf8', env: { ...process.env, GITHUB_ACTIONS: 'false', npm_execpath: '/must-not-execute' },
  });
  assert.equal(result.status, 1);
  assert.match(result.stderr, /AssertionError/);
  assert.doesNotMatch(result.stderr, /must-not-execute/);
});
