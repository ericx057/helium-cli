const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const childProcess = require('node:child_process');

function resolveCargo(env = process.env, home = os.homedir()) {
  const candidates = [
    ...(env.PATH || '').split(path.delimiter).filter(Boolean).map((dir) => path.join(dir, 'cargo')),
    path.join(home, '.cargo', 'bin', 'cargo'),
  ];
  for (const candidate of candidates) {
    try {
      fs.accessSync(candidate, fs.constants.X_OK);
      if (fs.statSync(candidate).isFile()) return candidate;
    } catch {
      // Continue to the standard Rust installation when PATH lacks cargo.
    }
  }
  throw new Error('Install Rust from https://rustup.rs, then run npm install again.');
}

function build(root, cargo) {
  const result = childProcess.spawnSync(cargo, ['build', '--release', '--locked', '--target-dir', path.join(root, 'target')], {
    cwd: root,
    stdio: 'inherit',
  });
  if (result.error) throw result.error;
  if (result.status !== 0) throw new Error('Rust build failed; review the compiler output above.');
}

function install() {
  if (os.platform() !== 'darwin') throw new Error('hel currently supports macOS only.');
  const root = path.resolve(__dirname, '..');
  build(root, resolveCargo());
  const destination = path.join(root, 'bin', 'hel-native');
  fs.copyFileSync(path.join(root, 'target', 'release', 'hel'), destination);
  fs.chmodSync(destination, 0o755);
  console.log('hel installed. Run hel --help to get started.');
}

if (require.main === module) {
  try {
    install();
  } catch (error) {
    console.error(`hel: ${error.message}`);
    process.exitCode = 1;
  }
}

module.exports = { resolveCargo, build, install };
