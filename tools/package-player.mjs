// Produce the independent companion web app with the official webOS CLI.
import fs from 'node:fs/promises';
import path from 'node:path';
import { spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';

const root = path.resolve(import.meta.dirname, '..');
const directory = path.join(root, 'extras/player/app');
const manifest = JSON.parse(await fs.readFile(path.join(directory, 'appinfo.json'), 'utf8'));
if (manifest.id !== 'com.denisefremov.luma.player' || manifest.type !== 'web') {
  throw new Error('Unexpected Luma Player identity or app type');
}
const output = path.join(root, 'dist');
await fs.mkdir(output, { recursive: true });
const state = path.join(root, '.build/cli-state');
await fs.mkdir(state, { recursive: true });
const result = spawnSync(
  process.execPath,
  [
    path.join(root, 'node_modules/@webos-tools/cli/bin/ares-package.js'),
    '--no-minify',
    '--outdir',
    output,
    directory,
  ],
  { cwd: root, env: { ...process.env, APPDATA: state }, stdio: 'inherit', timeout: 120000 },
);
if (result.error) throw result.error;
if (result.status !== 0) process.exit(result.status ?? 1);
const filename = `${manifest.id}_${manifest.version}_all.ipk`;
const bytes = await fs.readFile(path.join(output, filename));
if (bytes.subarray(0, 8).toString() !== '!<arch>\n') throw new Error('Invalid Luma Player IPK');
const hash = createHash('sha256').update(bytes).digest('hex');
const checksumsPath = path.join(output, 'sha256sums');
const previous = await fs.readFile(checksumsPath, 'utf8');
const nativeLines = previous
  .split('\n')
  .filter((line) => line.trim() && !line.includes(`  ${manifest.id}_`));
await fs.writeFile(checksumsPath, [...nativeLines, `${hash}  ${filename}`].join('\n') + '\n');
console.log(`Companion: dist/${filename} (${bytes.length} bytes) SHA-256: ${hash}`);
