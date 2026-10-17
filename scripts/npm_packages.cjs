'use strict';
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const crypto = require('node:crypto');
const { execFileSync } = require('node:child_process');
const root = path.resolve(__dirname, '..');
const source = require('../npm/package.json');
const suffixes = ['linux-x64', 'win32-x64', 'darwin-arm64', 'darwin-x64'];
const output = path.join(root, 'npm-dist', 'tarballs');

function npm(args, cwd = root) {
  assert(process.env.npm_execpath, 'Run this helper through npm run');
  return execFileSync(process.execPath, [process.env.npm_execpath, ...args], {
    cwd, encoding: 'utf8', stdio: ['ignore', 'pipe', 'pipe'],
    env: { ...process.env, npm_config_cache: path.join(root, 'npm-dist', 'cache') },
  });
}

function pack(directory) {
  fs.mkdirSync(output, { recursive: true });
  const [result] = JSON.parse(npm(['pack', directory, '--json', '--ignore-scripts', '--offline', '--pack-destination', output]));
  const manifest = JSON.parse(fs.readFileSync(path.join(directory, 'package.json'), 'utf8'));
  const expected = new Set(['package.json', 'LICENSE', 'README.md', ...manifest.files]);
  assert.deepEqual(new Set(result.files.map(file => file.path)), expected, 'Unexpected npm package contents');
  const file = path.join(output, result.filename);
  return { name: manifest.name, version: manifest.version, filename: result.filename,
    sha256: crypto.createHash('sha256').update(fs.readFileSync(file)).digest('hex') };
}

function verifyPrepared(directory = output) {
  const records = JSON.parse(fs.readFileSync(path.join(directory, 'packages.json'), 'utf8'));
  const names = [...suffixes.map(suffix => `${source.name}-${suffix}`), source.name];
  assert.deepEqual(records.map(record => record.name), names, 'Publish native packages before the launcher');
  assert.deepEqual(fs.readdirSync(directory).sort(), ['packages.json', ...records.map(record => record.filename)].sort());
  for (const record of records) {
    assert.equal(record.version, source.version);
    assert.equal(path.basename(record.filename), record.filename);
    assert.equal(crypto.createHash('sha256').update(fs.readFileSync(path.join(directory, record.filename))).digest('hex'), record.sha256);
  }
  return records;
}

function main() {
  const mode = process.argv[2];
  if (mode === 'pack') {
    const records = [...suffixes, 'launcher'].map(dir => pack(path.join(root, 'npm-dist', 'packages', dir)));
    fs.writeFileSync(path.join(output, 'packages.json'), JSON.stringify(records, null, 2) + '\n');
    for (const record of verifyPrepared()) {
      npm(['publish', path.join(output, record.filename), '--dry-run', '--ignore-scripts', '--offline', '--access', 'public', '--json']);
    }
    console.log('Packed and dry-run checked five npm packages. Nothing uploaded.');
  } else if (mode === 'stage') {
    // Defense in depth: no branch/tag/PR event or default manual run can upload.
    assert.equal(process.env.GITHUB_ACTIONS, 'true');
    assert.equal(process.env.GITHUB_EVENT_NAME, 'workflow_dispatch');
    assert.equal(process.env.NPM_STAGING_ENABLED, 'true');
    assert.equal(process.env.NPM_STAGE_CONFIRMATION, 'stage-reviewed-packages');
    assert.equal(process.env.GITHUB_REF, `refs/tags/v${source.version}`);
    assert(!process.env.NODE_AUTH_TOKEN && !process.env.NPM_TOKEN, 'Use OIDC, not a static npm token');
    for (const record of verifyPrepared()) {
      npm(['stage', 'publish', path.join(output, record.filename), '--ignore-scripts', '--access', 'public', '--registry', 'https://registry.npmjs.org/']);
      console.log(`Staged ${record.name}@${record.version}; maintainer approval is still required on npm.`);
    }
  } else {
    throw new Error('Expected pack or stage');
  }
}

module.exports = { npm, pack, verifyPrepared, output };
if (require.main === module) main();
