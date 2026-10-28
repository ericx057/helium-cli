#!/usr/bin/env node
const path = require('node:path');
const childProcess = require('node:child_process');

function launch(args) {
  const result = childProcess.spawnSync(path.join(__dirname, 'hel-native'), args, {
    stdio: 'inherit',
  });
  if (result.error) {
    console.error(`hel: could not start the native executable: ${result.error.message}. Run npm rebuild --global @ericx057/helium-cli.`);
    return 1;
  }
  return result.status ?? 1;
}

if (require.main === module) {
  process.exitCode = launch(process.argv.slice(2));
}

module.exports = { launch };
