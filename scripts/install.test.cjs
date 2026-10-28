const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const childProcess = require('node:child_process');
const { test } = require('node:test');
const { resolveCargo, build, install } = require('./install.cjs');
const { launch } = require('../bin/hel.cjs');
const { uninstall } = require('./delete.cjs');

test('finds an executable cargo on PATH or in the standard Rust directory', () => {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'hel-install-test-'));
  try {
    const cargoDir = path.join(root, '.cargo', 'bin');
    fs.mkdirSync(cargoDir, { recursive: true });
    const cargo = path.join(cargoDir, 'cargo');
    fs.writeFileSync(cargo, '', { mode: 0o755 });
    assert.equal(resolveCargo({ PATH: '' }, root), cargo);
    assert.equal(resolveCargo({ PATH: cargoDir }, '/missing'), cargo);
    assert.throws(() => resolveCargo({ PATH: '' }, '/missing'), /Install Rust/);
  } finally {
    fs.rmSync(root, { recursive: true, force: true });
  }
});

test('build runs locked release compilation without a shell', (t) => {
  const calls = [];
  t.mock.method(childProcess, 'spawnSync', (...args) => {
    calls.push(args);
    return { status: 0 };
  });
  build('/package', '/cargo');
  assert.deepEqual(calls, [[
    '/cargo', ['build', '--release', '--locked', '--target-dir', '/package/target'],
    { cwd: '/package', stdio: 'inherit' },
  ]]);
});

test('failed builds and process startup errors stop installation', (t) => {
  const mock = t.mock.method(childProcess, 'spawnSync', () => ({ status: 1 }));
  assert.throws(() => build('/package', '/cargo'), /build failed/);
  mock.mock.mockImplementation(() => ({ error: new Error('denied') }));
  assert.throws(() => build('/package', '/cargo'), /denied/);
  mock.mock.mockImplementation(() => ({ status: null, signal: 'SIGTERM' }));
  assert.throws(() => build('/package', '/cargo'), /build failed/);
});

test('installation rejects unsupported platforms before accessing files', (t) => {
  t.mock.method(os, 'platform', () => 'linux');
  assert.throws(() => install(), /macOS/);
});

test('successful installation copies and marks the native binary executable', (t) => {
  t.mock.method(os, 'platform', () => 'darwin');
  t.mock.method(fs, 'accessSync', () => {});
  t.mock.method(fs, 'statSync', () => ({ isFile: () => true }));
  t.mock.method(childProcess, 'spawnSync', () => ({ status: 0 }));
  const copies = [];
  const modes = [];
  t.mock.method(fs, 'copyFileSync', (...args) => copies.push(args));
  t.mock.method(fs, 'chmodSync', (...args) => modes.push(args));
  t.mock.method(console, 'log', () => {});
  install();
  const root = path.resolve(__dirname, '..');
  assert.deepEqual(copies, [[
    path.join(root, 'target', 'release', 'hel'), path.join(root, 'bin', 'hel-native'),
  ]]);
  assert.deepEqual(modes, [[path.join(root, 'bin', 'hel-native'), 0o755]]);
});

test('wrapper forwards arguments literally and propagates native status', (t) => {
  const calls = [];
  t.mock.method(childProcess, 'spawnSync', (...args) => {
    calls.push(args);
    return { status: 7 };
  });
  assert.equal(launch(['tab', 'quotes " and $(touch /tmp/no)']), 7);
  assert.equal(calls[0][0], path.resolve(__dirname, '../bin/hel-native'));
  assert.deepEqual(calls[0][1], ['tab', 'quotes " and $(touch /tmp/no)']);
  assert.deepEqual(calls[0][2], { stdio: 'inherit' });
});

test('wrapper reports a missing native executable', (t) => {
  t.mock.method(childProcess, 'spawnSync', () => ({ error: new Error('ENOENT') }));
  const errors = [];
  t.mock.method(console, 'error', (message) => errors.push(message));
  assert.equal(launch([]), 1);
  assert.match(errors[0], /npm rebuild/);
});

test('delete invokes only the global package uninstall without a shell', (t) => {
  const calls = [];
  t.mock.method(childProcess, 'spawnSync', (...args) => {
    calls.push(args);
    return { status: 0 };
  });
  assert.equal(uninstall(), 0);
  const npmCli = process.env.npm_execpath;
  assert.deepEqual(calls, [[
    npmCli ? process.execPath : 'npm',
    [...(npmCli ? [npmCli] : []), 'uninstall', '--global', '@ericx057/helium-cli'],
    { stdio: 'inherit' },
  ]]);
});
