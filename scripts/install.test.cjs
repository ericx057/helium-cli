const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const childProcess = require('node:child_process');
const { test } = require('node:test');
const { resolveCargo, build, install } = require('./install.cjs');
const { uninstall } = require('./delete.cjs');

function launcherFixture(t) {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'hel launcher test '));
  t.after(() => fs.rmSync(root, { recursive: true, force: true }));
  const directory = path.join(root, 'package with spaces', 'bin');
  fs.mkdirSync(directory, { recursive: true });
  const launcher = path.join(directory, 'hel');
  fs.copyFileSync(path.resolve(__dirname, '../bin/hel'), launcher);
  fs.chmodSync(launcher, 0o755);
  const native = path.join(directory, 'hel-native');
  fs.writeFileSync(native, '#!/bin/sh\nprintf \'%s\\0\' "$@"\nexit 7\n', { mode: 0o755 });
  return { root, launcher, native };
}

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

test('shell launcher forwards arguments literally and propagates native status', (t) => {
  const { root, launcher } = launcherFixture(t);
  const marker = path.join(root, 'must-not-exist');
  const args = ['tab', `quotes " and $(touch "${marker}")`, '', '*.txt', '東京'];
  const result = childProcess.spawnSync(launcher, args, { encoding: 'utf8' });
  assert.equal(result.status, 7);
  assert.equal(result.stdout, `${args.join('\0')}\0`);
  assert.equal(fs.existsSync(marker), false);
});

test('launcher resolves absolute and relative symlinks through paths with spaces', (t) => {
  const { root, launcher } = launcherFixture(t);
  const links = path.join(root, 'links with spaces');
  fs.mkdirSync(links);
  const absolute = path.join(links, 'absolute');
  const relative = path.join(links, 'relative');
  const chain = path.join(links, 'chain');
  fs.symlinkSync(launcher, absolute);
  fs.symlinkSync(path.relative(links, launcher), relative);
  fs.symlinkSync('relative', chain);
  for (const command of [absolute, relative, chain, './chain']) {
    const result = childProcess.spawnSync(command, ['literal title'], {
      encoding: 'utf8', cwd: links,
    });
    assert.equal(result.status, 7, result.stderr);
    assert.equal(result.stdout, 'literal title\0');
  }
});

test('launcher reports a missing native executable with status 127', (t) => {
  const { launcher, native } = launcherFixture(t);
  fs.unlinkSync(native);
  const result = childProcess.spawnSync(launcher, [], { encoding: 'utf8' });
  assert.equal(result.status, 127);
  assert.match(result.stderr, /npm rebuild --global @ericx057\/helium-cli/);
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
