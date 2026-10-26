// NOPE weather sky: bake blur, shadow and glow ONCE. Only transforms/tints at runtime.
import fs from 'node:fs/promises';
import path from 'node:path';
import sharp from 'sharp';

const output = path.resolve(import.meta.dirname, '..', '.build', 'assets', 'weather-sky');
await fs.mkdir(output, { recursive: true });

const make = async (name, width, height, drawing) => {
  const svg = Buffer.from(
    `<svg xmlns="http://www.w3.org/2000/svg" width="${width}" height="${height}" viewBox="0 0 ${width} ${height}">${drawing}</svg>`,
  );
  await sharp(svg)
    .png()
    .toFile(path.join(output, `${name}.png`));
};
// Smooth clouds (approved mockup https://claude.ai/artifact/9tx7pF7PJUe4GaD435VoNZ):
// a crisp union of circles and capsules in a 240x120 box, blurred wide and
// cut with a soft one-pixel threshold at 4x, then scaled down. Concave joins
// get round fillets and the edge stays sharp, smooth and anti-aliased.
const CLOUDS = {
  flat: [
    {
      c: [
        [70, 74, 30],
        [118, 62, 40],
        [168, 78, 24],
      ],
      p: [[18, 70, 206, 40]],
    },
    {
      c: [
        [84, 70, 32],
        [144, 72, 28],
      ],
      p: [[30, 72, 182, 38]],
    },
    {
      c: [
        [64, 78, 22],
        [104, 64, 34],
        [156, 66, 34],
        [196, 82, 18],
      ],
      p: [[22, 74, 200, 36]],
    },
    {
      c: [
        [96, 66, 36],
        [150, 76, 26],
      ],
      p: [[40, 74, 170, 36]],
    },
  ],
  classic: [
    {
      c: [
        [66, 70, 32],
        [116, 50, 44],
        [170, 70, 30],
      ],
      p: [[24, 64, 192, 46]],
    },
    {
      c: [
        [72, 62, 38],
        [138, 56, 40],
        [186, 78, 24],
      ],
      p: [[28, 68, 186, 42]],
    },
    {
      c: [
        [60, 74, 26],
        [102, 56, 38],
        [150, 50, 40],
        [194, 76, 22],
      ],
      p: [[22, 70, 196, 40]],
    },
    {
      c: [
        [84, 60, 36],
        [146, 66, 32],
      ],
      p: [[34, 68, 170, 42]],
    },
  ],
  // Approved 25.10: one capsule base and two circles, flat tone, crisp edge
  // (Settings illustration mockup https://claude.ai/artifact/496bUST8uTdCx7tTfCgXVn).
  pill: [
    {
      c: [
        [76, 68, 44],
        [144, 56, 56],
      ],
      p: [[0, 56, 240, 64]],
    },
    {
      c: [
        [82, 70, 40],
        [146, 62, 50],
      ],
      p: [[10, 62, 220, 58]],
    },
    {
      c: [
        [96, 56, 56],
        [164, 68, 44],
      ],
      p: [[0, 56, 240, 64]],
    },
    {
      c: [
        [88, 70, 38],
        [148, 66, 44],
      ],
      p: [[18, 66, 204, 54]],
    },
  ],
  capsule: [
    {
      c: [],
      p: [
        [20, 76, 200, 34],
        [58, 46, 120, 34],
      ],
    },
    {
      c: [],
      p: [
        [16, 78, 170, 32],
        [50, 50, 150, 32],
        [100, 22, 70, 30],
      ],
    },
    {
      c: [],
      p: [
        [30, 74, 180, 36],
        [74, 44, 96, 34],
      ],
    },
    {
      c: [],
      p: [
        [24, 72, 192, 36],
        [90, 42, 110, 34],
      ],
    },
  ],
};
const CLOUD_STYLE = 'pill';
// The pill style keeps its geometric edge and a single flat tone.
const FLAT = CLOUD_STYLE === 'pill';
const PAD = 20;
const bakeCloud = async (shape, file) => {
  const k = 4,
    w = 240 + PAD * 2,
    h = 120 + PAD * 2;
  const body =
    shape.c.map(([x, y, r]) => `<circle cx="${x + PAD}" cy="${y + PAD}" r="${r}"/>`).join('') +
    shape.p
      .map(
        ([x, y, pw, ph]) =>
          `<rect x="${x + PAD}" y="${y + PAD}" width="${pw}" height="${ph}" rx="${ph / 2}"/>`,
      )
      .join('');
  const svg = Buffer.from(
    `<svg xmlns="http://www.w3.org/2000/svg" width="${w * k}" height="${h * k}" viewBox="0 0 ${w} ${h}"><defs><filter id="b" x="-20%" y="-40%" width="140%" height="180%"><feGaussianBlur stdDeviation="${FLAT ? 2 : 7}"/></filter></defs><g fill="#fff" filter="url(#b)">${body}</g></svg>`,
  );
  const { data, info } = await sharp(svg).ensureAlpha().raw().toBuffer({ resolveWithObject: true });
  for (let y = 0; y < info.height; y += 1) {
    // Top-to-bottom shade inside the silhouette: white, then a cool light grey.
    const v = Math.min(1, Math.max(0, ((y / k - PAD) / 112 - 0.35) / 0.65));
    const rgb = FLAT ? [242, 245, 250] : [255 - 36 * v, 255 - 25 * v, 255 - 14 * v];
    for (let x = 0; x < info.width; x += 1) {
      const i = (y * info.width + x) * 4;
      const t = Math.min(1, Math.max(0, (data[i + 3] / 255 - 0.47) / 0.06));
      data[i] = rgb[0];
      data[i + 1] = rgb[1];
      data[i + 2] = rgb[2];
      data[i + 3] = Math.round(255 * t * t * (3 - 2 * t));
    }
  }
  await sharp(data, { raw: info })
    .resize(w * 2, h * 2, { kernel: 'lanczos3' })
    .png()
    .toFile(path.join(output, file));
};
for (const [index, shape] of CLOUDS[CLOUD_STYLE].entries()) {
  await bakeCloud(shape, index ? `cloud${index + 1}.png` : 'cloud.png');
}
// Sun rays from beyond the top-right corner of the card (566x372, baked at
// 1x with the card's aspect); two fans offset by half a beam cross-fade.
const rays = (offset) => {
  const ox = 566 * 1.05,
    oy = -372 * 0.09,
    len = Math.hypot(566, 372) * 1.05;
  const beams = [];
  for (let i = 0; i < 8; i += 1) {
    const a = ((98 + i * 12 + offset) * Math.PI) / 180;
    const half = ((i % 2 ? 2.2 : 3.6) * Math.PI) / 180;
    const p = (angle) =>
      `${(ox + len * Math.cos(angle)).toFixed(1)} ${(oy + len * Math.sin(angle)).toFixed(1)}`;
    beams.push(`<path d="M${ox} ${oy}L${p(a - half)}L${p(a + half)}Z"/>`);
  }
  return `<defs><radialGradient id="f" cx="${ox}" cy="${oy}" r="${len}" gradientUnits="userSpaceOnUse"><stop offset=".05" stop-color="#fffceb" stop-opacity=".5"/><stop offset=".55" stop-color="#fffceb" stop-opacity=".16"/><stop offset="1" stop-color="#fffceb" stop-opacity="0"/></radialGradient><radialGradient id="g" cx="${ox}" cy="${oy}" r="${566 * 0.55}" gradientUnits="userSpaceOnUse"><stop stop-color="#fffae1" stop-opacity=".55"/><stop offset="1" stop-color="#fffae1" stop-opacity="0"/></radialGradient><filter id="b"><feGaussianBlur stdDeviation="6"/></filter></defs><g fill="url(#f)" filter="url(#b)">${beams.join('')}</g>${offset ? '' : '<rect width="566" height="372" fill="url(#g)"/>'}`;
};
await make('rays', 566, 372, rays(0));
await make('rays2', 566, 372, rays(6));
// Atmospheric banks and sun rings are pre-blurred RGBA.
await make(
  'fog',
  256,
  64,
  '<defs><filter id="b"><feGaussianBlur stdDeviation="14"/></filter></defs><rect x="23" y="21" width="210" height="22" rx="11" fill="white" opacity=".5" filter="url(#b)"/>',
);
await make(
  'glow',
  256,
  256,
  '<defs><radialGradient id="g"><stop stop-color="white" stop-opacity=".34"/><stop offset=".42" stop-color="white" stop-opacity=".18"/><stop offset=".72" stop-color="white" stop-opacity=".08"/><stop offset="1" stop-color="white" stop-opacity="0"/></radialGradient></defs><circle cx="128" cy="128" r="127" fill="url(#g)"/>',
);
await make(
  'dot',
  24,
  24,
  '<defs><radialGradient id="g"><stop stop-color="white"/><stop offset=".24" stop-color="white" stop-opacity=".8"/><stop offset="1" stop-color="white" stop-opacity="0"/></radialGradient></defs><circle cx="12" cy="12" r="12" fill="url(#g)"/>',
);
await make(
  'rain',
  12,
  38,
  '<path d="M8 3L3 35" fill="none" stroke="white" stroke-width="3.2" stroke-linecap="round" opacity=".72"/>',
);
await make(
  'snow',
  24,
  24,
  '<defs><radialGradient id="g"><stop stop-color="white"/><stop offset=".55" stop-color="white" stop-opacity=".84"/><stop offset="1" stop-color="white" stop-opacity="0"/></radialGradient></defs><circle cx="12" cy="12" r="12" fill="url(#g)"/>',
);
await make(
  'wind',
  220,
  26,
  '<defs><linearGradient id="g"><stop stop-color="white" stop-opacity="0"/><stop offset=".5" stop-color="white" stop-opacity=".7"/><stop offset="1" stop-color="white" stop-opacity="0"/></linearGradient></defs><path d="M3 14C78 -8 144 33 217 10" stroke="url(#g)" stroke-width="2" fill="none" stroke-linecap="round"/>',
);
await make(
  'bolt',
  80,
  210,
  '<defs><filter id="g"><feGaussianBlur stdDeviation="5"/></filter></defs><path d="M46 4L26 45 43 80 16 117 30 154 12 205" stroke="white" stroke-width="8" opacity=".48" fill="none" filter="url(#g)"/><path d="M46 4L26 45 43 80 16 117 30 154 12 205" stroke="white" stroke-width="3" opacity=".97" fill="none" stroke-linejoin="round"/>',
);
// No opaque disc: transparent crescent SVG path plus glow.
await make(
  'moon',
  128,
  128,
  '<defs><filter id="b"><feGaussianBlur stdDeviation="8"/></filter></defs><path d="M77 14A46 46 0 1 0 114 92A47 47 0 0 1 77 14Z" fill="#e8f5ff" opacity=".55" filter="url(#b)"/><path d="M77 14A46 46 0 1 0 114 92A47 47 0 0 1 77 14Z" fill="#e8f5ff"/>',
);
console.log('Weather sky sprites generated:', output);
