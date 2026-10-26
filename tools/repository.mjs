import fs from 'node:fs/promises';
import path from 'node:path';
import { createHash } from 'node:crypto';

const root = path.resolve(import.meta.dirname, '..');
const manifest = JSON.parse(await fs.readFile(path.join(root, 'appinfo.json'), 'utf8'));
const pkg = JSON.parse(await fs.readFile(path.join(root, 'package.json'), 'utf8'));
if (!/^\d+\.\d+\.\d+$/.test(manifest.version) || pkg.version !== manifest.version)
  throw new Error('package.json and appinfo.json must have the same numeric release version.');
const tag = `v${manifest.version}`;
const requestedTag =
  process.env.RELEASE_TAG ||
  (process.env.GITHUB_REF_TYPE === 'tag' ? process.env.GITHUB_REF_NAME : '');
if (requestedTag && requestedTag !== tag)
  throw new Error(`Release tag must be ${tag}, matching the packaged app version.`);

const repository = process.env.GITHUB_REPOSITORY || 'funnymataleao/luma-webos-launcher';
if (!/^[\w.-]+\/[\w.-]+$/.test(repository)) throw new Error('Invalid GitHub repository name.');
const sourceUrl = `https://github.com/${repository}`;
const releaseUrl = `${sourceUrl}/releases/latest/download`;
const filename = `${manifest.id}_${manifest.version}_arm.ipk`;
const output = path.join(root, 'dist');
const bytes = await fs.readFile(path.join(output, filename));
const iconUri = `https://raw.githubusercontent.com/${repository}/main/assets/app-icon-large.png`;
const descriptionFilename = 'description.html';
const shortDescription = manifest.appDescription?.trim();
if (!shortDescription) throw new Error('appinfo.json must provide a non-empty appDescription.');
const player = JSON.parse(
  await fs.readFile(path.join(root, 'extras/player/app/appinfo.json'), 'utf8'),
);
if (player.id !== 'com.denisefremov.luma.player' || player.type !== 'web') {
  throw new Error('Companion player manifest does not match the public identity.');
}
const playerFilename = `${player.id}_${player.version}_all.ipk`;
const playerBytes = await fs.readFile(path.join(output, playerFilename));
const playerIcon = `https://raw.githubusercontent.com/${repository}/main/extras/player/app/icon.png`;
const playerDescription = 'Companion video player for Jellyfin and Plex media playback in Luma.';
const playerDescriptionFilename = 'player-description.html';
const playerManifest = {
  id: player.id,
  version: player.version,
  type: player.type,
  title: player.title,
  appDescription: playerDescription,
  iconUri: playerIcon,
  sourceUrl,
  rootRequired: false,
  ipkUrl: `${releaseUrl}/${playerFilename}`,
  ipkHash: { sha256: createHash('sha256').update(playerBytes).digest('hex') },
  ipkSize: playerBytes.length,
};
const feed = {
  paging: { page: 1, count: 2, maxPage: 1, itemsTotal: 2 },
  packages: [
    {
      id: manifest.id,
      title: manifest.title,
      shortDescription,
      fullDescriptionUrl: `${releaseUrl}/${descriptionFilename}`,
      iconUri,
      manifest: {
        id: manifest.id,
        version: manifest.version,
        type: manifest.type,
        title: manifest.title,
        appDescription: manifest.appDescription,
        iconUri,
        sourceUrl,
        rootRequired: false,
        ipkUrl: `${releaseUrl}/${filename}`,
        ipkHash: { sha256: createHash('sha256').update(bytes).digest('hex') },
        ipkSize: bytes.length,
      },
    },
    {
      id: player.id,
      title: player.title,
      shortDescription: playerDescription,
      fullDescriptionUrl: `${releaseUrl}/${playerDescriptionFilename}`,
      iconUri: playerIcon,
      manifest: playerManifest,
    },
  ],
};
await fs.writeFile(
  path.join(output, `${player.id}.manifest.json`),
  JSON.stringify(playerManifest, null, 2) + '\n',
);
await fs.writeFile(
  path.join(output, `${manifest.id}.manifest.json`),
  JSON.stringify(feed.packages[0].manifest, null, 2) + '\n',
);
await fs.copyFile(
  path.join(root, '.build/package', manifest.largeIcon),
  path.join(output, manifest.largeIcon),
);
await fs.copyFile(
  path.join(root, 'assets/homebrew-description.html'),
  path.join(output, descriptionFilename),
);
await fs.copyFile(
  path.join(root, 'assets', playerDescriptionFilename),
  path.join(output, playerDescriptionFilename),
);
await fs.writeFile(path.join(output, 'repo.json'), JSON.stringify(feed, null, 2) + '\n');
if (process.env.GITHUB_OUTPUT) await fs.appendFile(process.env.GITHUB_OUTPUT, `tag=${tag}\n`);
console.log(`Homebrew feed: dist/repo.json (${tag}, ${bytes.length} byte IPK)`);
