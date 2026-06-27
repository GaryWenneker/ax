/**
 * Build transparent logo.png from assets/logo.jpg:
 * - Remove solid black (or near-black) background
 * - Preserve cyan network glow and letter highlights
 */
import sharp from 'sharp';
import { fileURLToPath } from 'url';
import { dirname, join } from 'path';

const root = dirname(fileURLToPath(import.meta.url));
const input = join(root, '../../assets/logo.jpg');
const output = join(root, '../public/logo.png');
const outputSource = join(root, '../../assets/logo.png');

const img = sharp(input);
const meta = await img.metadata();
const w = meta.width ?? 0;
const h = meta.height ?? 0;

const { data } = await img.ensureAlpha().raw().toBuffer({ resolveWithObject: true });

/** True when pixel is backdrop black (not letter interior). */
function isBackground(r, g, b) {
	const max = Math.max(r, g, b);
	const lum = (r + g + b) / 3;

	// Solid black / near-black backdrop from the new source art
	if (max <= 22) {
		return true;
	}

	// Residual dark matte without cyan glow (legacy grey exports)
	const isGlow = b > r + 18 && g > r + 5;
	const isEdgeHighlight = lum > 140;
	if (!isGlow && !isEdgeHighlight && lum < 62 && max < 48) {
		return true;
	}

	return false;
}

for (let y = 0; y < h; y++) {
	for (let x = 0; x < w; x++) {
		const i = (y * w + x) * 4;
		const r = data[i];
		const g = data[i + 1];
		const b = data[i + 2];

		if (isBackground(r, g, b)) {
			data[i + 3] = 0;
		}
	}
}

const png = sharp(data, { raw: { width: w, height: h, channels: 4 } }).png();
await png.toFile(output);
await png.toFile(outputSource);
console.log(`Wrote ${output} and ${outputSource} (${w}x${h})`);
