// Opens one temporary window; closes only that window and its test tabs.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const { spawnSync } = require('node:child_process');

const root = path.resolve(__dirname, '..');
const binary = process.env.HEL_BINARY || path.join(root, 'target/release/hel');
const state = fs.mkdtempSync(path.join(os.tmpdir(), 'hel-smoke-'));
const env = { ...process.env, XDG_STATE_HOME: state };
const script = fs.readFileSync(path.join(root, 'src/helium.js'), 'utf8');

function automation(action, ...args) {
  const result = spawnSync('/usr/bin/osascript', ['-l', 'JavaScript', '-e', script, '--', action, ...args], { encoding: 'utf8' });
  assert.equal(result.status, 0, 'automation command failed');
  const data = JSON.parse(result.stdout);
  assert.equal(data.error, undefined, 'automation returned an error');
  return data;
}

function hel(...args) {
  const result = spawnSync(binary, args, { encoding: 'utf8', env });
  assert.equal(result.status, 0, `hel ${args[0] || ''} failed: ${result.stderr}`);
  return result.stdout;
}

function saved() {
  return JSON.parse(fs.readFileSync(path.join(state, 'hel/state.json'), 'utf8'));
}

const before = automation('snapshot');
let owned;
try {
  hel('open');
  owned = saved();
  assert.equal(owned.opened_windows.length, 1);
  const window = owned.opened_windows[0];
  hel('new');
  const second = saved().opened_tabs.at(-1);
  assert.equal(second.window_id, window, 'new tab used the wrong window');
  assert.ok(hel('list').includes(`window ${window}\t`));
  const number = saved().order.findIndex((tab) => tab.tab_id === second.tab_id) + 1;
  hel('tab', String(number));
  hel('hel CLI integration verification');
  assert.ok(hel('search', 'duckduckgo').includes(`window ${window}\t`));
  hel('close');
  hel('close');
  hel('quit');
  const after = automation('snapshot');
  assert.equal(after.session, before.session, 'browser unexpectedly restarted');
  const ids = (snapshot) => snapshot.tabs.map((tab) => tab.tab_id).sort((a, b) => a - b);
  assert.deepEqual(ids(after), ids(before), 'original browser tabs changed');
  console.log('Live smoke passed: open, new, list, focus, query, site search, close, quit; original tabs preserved.');
} finally {
  if (owned) {
    const live = automation('snapshot');
    const window = owned.opened_windows[0];
    if (live.session === owned.session && live.tabs.some((tab) => tab.window_id === window)) {
      automation('close-window', owned.session, String(window));
    }
  }
  fs.rmSync(state, { recursive: true, force: true });
}
