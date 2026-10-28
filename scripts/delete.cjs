const childProcess = require('node:child_process');

function uninstall() {
  const npmCli = process.env.npm_execpath;
  const result = childProcess.spawnSync(
    npmCli ? process.execPath : 'npm',
    [...(npmCli ? [npmCli] : []), 'uninstall', '--global', '@ericx057/helium-cli'],
    { stdio: 'inherit' },
  );
  if (result.error) {
    console.error(`hel: uninstall failed: ${result.error.message}`);
    return 1;
  }
  return result.status ?? 1;
}

if (require.main === module) process.exitCode = uninstall();

module.exports = { uninstall };
