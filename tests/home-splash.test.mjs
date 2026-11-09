import assert from 'node:assert/strict';
import fs from 'node:fs/promises';
import test from 'node:test';
import vm from 'node:vm';

const file = (name) => fs.readFile(new URL(`../tv/${name}`, import.meta.url), 'utf8');

// Public builds never replace LG's original Home or alter system mounts.
test('the system-home helper refuses takeover without invoking native commands', async () => {
  const source = await file('home.cjs');
  const calls = [];
  const context = vm.createContext({
    console: { error: (message) => calls.push(message) },
    process: { exit: (status) => calls.push(status) },
    require: () => assert.fail('Home takeover helper must not read system modules'),
  });
  vm.runInContext(source, context, { timeout: 2000 });
  assert.deepEqual(calls, ['Luma: home takeover intentionally disabled.', 1]);
});

test('the shell home helper deliberately refuses takeover', async () => {
  const source = await file('home.sh');
  assert.match(source, /system takeover and autostart are intentionally disabled/);
  assert.match(source, /^exit 1$/m);
});

test('autostart is an explicit opt-in boot hook for the public launcher id', async () => {
  const source = await file('autostart.sh');
  assert.match(source, /^APP_ID=com\.archbtw\.luma$/m);
  assert.match(source, /HOOK=\/var\/lib\/webosbrew\/init\.d\/65-luma/);
  assert.match(source, /^  enable\)$/m);
  assert.match(source, /^  disable\)$/m);
  assert.match(source, /flock -n 9/);
  assert.match(source, /luna:\/\/com\.webos\.service\.applicationmanager\/launch/);
  assert.doesNotMatch(source, /com\.ecklf\.luma/);
});

test('wake script targets the public launcher id and not the original beta app', async () => {
  const source = await file('luma-wake.sh');
  assert.match(source, /com\.archbtw\.luma/);
  assert.match(source, /luna-send/);
});
