import fs from 'node:fs/promises';
import path from 'node:path';
import { spawnSync } from 'node:child_process';

const root = path.resolve(import.meta.dirname, '..');
const deviceIndex = process.argv.indexOf('--device');
const device = deviceIndex >= 0 ? process.argv[deviceIndex + 1] : 'tv';
if (!device || !/^[a-zA-Z0-9_-]+$/.test(device))
  throw new Error('Pass an ares device profile: pnpm run deploy --device tv');
const manifest = JSON.parse(await fs.readFile(path.join(root, 'appinfo.json'), 'utf8'));
for (const [tool, args] of [
  [
    'ares-install',
    ['--device', device, path.join(root, 'dist', `${manifest.id}_${manifest.version}_arm.ipk`)],
  ],
  ['ares-launch', ['--device', device, manifest.id]],
]) {
  const cli = path.join(root, 'node_modules/@webos-tools/cli/bin', `${tool}.js`);
  const result = spawnSync(process.execPath, [cli, ...args], {
    cwd: root,
    stdio: 'inherit',
    timeout: 120000,
  });
  if (result.error) throw result.error;
  if (result.status !== 0) process.exit(result.status ?? 1);
}
