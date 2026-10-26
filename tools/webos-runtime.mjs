// SDL in the NDK is for linking, not TV display: its Wayland backend is disabled.
import fs from 'node:fs/promises';
import path from 'node:path';
import { createHash } from 'node:crypto';
import { spawnSync } from 'node:child_process';
import { pathToFileURL } from 'node:url';

const root = path.resolve(import.meta.dirname, '..');
const directory = path.join(root, '.build/sdl-webos');
const release = 'release-2.30.12-webos.5';
const filename = 'SDL2-2.30.12-webos.tar.gz';
const archiveHash = '4ad566453d113bdd9ee96878176b97d28d9fa70503a62d83d55a351545abb334';
const libraryHash = 'f564f7da624cec47168300d017f3ec822f41b5096f0ce1c96873eecd8614d446';
const hash = (bytes) => createHash('sha256').update(bytes).digest('hex');

export function verifyWebosSdl(bytes) {
  if (
    bytes.length < 52 ||
    bytes.subarray(0, 4).toString('hex') !== '7f454c46' ||
    bytes[4] !== 1 ||
    bytes[5] !== 1 ||
    bytes.readUInt16LE(18) !== 40 ||
    bytes.readUInt16LE(16) !== 3 ||
    (bytes.readUInt32LE(36) & 0x400) !== 0
  ) {
    throw new Error('SDL runtime must be an ARM soft-float shared library.');
  }
  // These are compiled backend strings, not merely exported SDL_webOS API stubs.
  if (
    !bytes.includes(Buffer.from('SDL Wayland video driver\0')) ||
    !bytes.includes(Buffer.from('APPID environment variable is not set\0'))
  ) {
    throw new Error('SDL has no webOS Wayland video backend. Do not package the SDK stub SDL.');
  }
}

export async function prepareWebosRuntime() {
  await fs.mkdir(directory, { recursive: true });
  const archive = path.join(directory, filename);
  let bytes;
  try {
    bytes = await fs.readFile(archive);
  } catch (error) {
    if (error.code !== 'ENOENT') throw error;
    console.log(`Downloading SDL-webOS ${release} (standard SDL ABI).`);
    const url = `https://github.com/webosbrew/SDL-webOS/releases/download/${release}/${filename}`;
    const response = await fetch(url, { signal: AbortSignal.timeout(60000) });
    if (!response.ok) throw new Error(`SDL-webOS download failed: ${response.status}`);
    bytes = Buffer.from(await response.arrayBuffer());
    if (hash(bytes) !== archiveHash) throw new Error('SDL-webOS archive checksum mismatch.');
    await fs.writeFile(archive, bytes);
  }
  if (hash(bytes) !== archiveHash) throw new Error('SDL-webOS archive checksum mismatch.');

  const library = path.join(directory, 'lib/libSDL2-2.0.so.0');
  let current;
  try {
    current = await fs.readFile(library);
  } catch (error) {
    if (error.code !== 'ENOENT') throw error;
  }
  if (!current || hash(current) !== libraryHash) {
    const result = spawnSync('tar', ['-xzf', archive, '-C', directory], {
      stdio: 'inherit',
      timeout: 60000,
    });
    if (result.error) throw result.error;
    if (result.status !== 0) throw new Error('SDL-webOS extraction failed.');
    current = await fs.readFile(library);
  }
  if (hash(current) !== libraryHash) throw new Error('SDL-webOS library checksum mismatch.');
  verifyWebosSdl(current);

  // Upstream metadata embeds /usr/local and an rpath. Generate relocatable build-host
  // lookup metadata instead; the TV executable keeps only its $ORIGIN/lib rpath.
  const pkgconfig = path.join(directory, 'pkgconfig');
  await fs.mkdir(pkgconfig, { recursive: true });
  await fs.writeFile(
    path.join(pkgconfig, 'sdl2.pc'),
    `prefix=${directory}
libdir=\${prefix}/lib
includedir=\${prefix}/include/SDL2
Name: sdl2
Description: SDL-webOS with the Wayland/webOS backend and standard SDL ABI
Version: 2.30.12
Libs: -L\${libdir} -lSDL2
Cflags: -I\${includedir} -D_REENTRANT
`,
  );
  console.log(`Verified SDL-webOS ${release}: ARM soft-float, Wayland/webOS backend.`);
  return directory;
}

if (process.argv[1] && import.meta.url === pathToFileURL(path.resolve(process.argv[1])).href) {
  await prepareWebosRuntime();
}
