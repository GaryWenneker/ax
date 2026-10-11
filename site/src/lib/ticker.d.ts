/** How many identical sequences the marquee needs so the loop never shows an empty end. */
export declare function tickerCopiesNeeded(sequenceWidth: number, viewportWidth: number, existing: number): number;
/** Sequences in one half of the track. The page clones that half so the loop can move -50%. */
export declare function tickerHalfCopies(sequenceWidth: number, viewportWidth: number): number;
export declare function tickerDurationSeconds(sequenceWidth: number): number;
