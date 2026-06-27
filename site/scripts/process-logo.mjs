/**
 * Build transparent logo.png from assets/logo.jpg:
 * - Remove dark grey background (alpha)
 * - Erase small AI star watermark bottom-right
 */
import sharp from 'sharp';
import { fileURLToPath } from 'url';
import { dirname, join } from 'path';

const root = dirname(fileURLToPath(import.meta.url));
const input = join(root, '../../assets/logo.jpg');
const output = join(root, '../public/logo.png');

const img = sharp(input);
const meta = await img.metadata();
const w = meta.width ?? 0;
const h = meta.height ?? 0;

const { data } = await img.ensureAlpha().raw().toBuffer({ resolveWithObject: true });

for (let y = 0; y < h; y++) {
	for (let x = 0; x < w; x++) {
		const i = (y * w + x) * 4;
		const r = data[i];
		const g = data[i + 1];
		const b = data[i + 2];

		// Bottom-right watermark star (~14% corner)
		if (x > w * 0.86 && y > h * 0.86) {
			data[i + 3] = 0;
			continue;
		}

		const lum = (r + g + b) / 3;
		const isGlow = b > r + 18 && g > r + 5;
		const isEdgeHighlight = lum > 140;

		// Drop matte grey backdrop; keep cyan network + letter highlights
		if (!isGlow && !isEdgeHighlight && lum < 62) {
			data[i + 3] = 0;
		}
	}
}

await sharp(data, { raw: { width: w, height: h, channels: 4 } }).png().toFile(output);
console.log(`Wrote ${output} (${w}x${h})`);
