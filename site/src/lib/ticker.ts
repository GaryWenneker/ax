/** How many identical sequences the marquee needs so the loop never shows an empty end. */

const MAX_COPIES = 40;
const PIXELS_PER_SECOND = 70;
const MIN_DURATION_SECONDS = 12;

export function tickerCopiesNeeded(
	sequenceWidth: number,
	viewportWidth: number,
	existing: number,
): number {
	if (!(sequenceWidth > 0) || !(viewportWidth > 0)) return existing;
	const copies = Math.ceil((viewportWidth + sequenceWidth) / sequenceWidth);
	return Math.min(MAX_COPIES, Math.max(existing, copies));
}

export function tickerDurationSeconds(sequenceWidth: number): number {
	if (!(sequenceWidth > 0)) return 20;
	return Math.max(MIN_DURATION_SECONDS, sequenceWidth / PIXELS_PER_SECOND);
}
