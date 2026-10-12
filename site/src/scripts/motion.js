/** Explicit Play/Pause overrides the OS preference; otherwise respect it. */
export function motionPaused() {
  const choice = document.documentElement.dataset.axMotion;
  return choice ? choice === "paused" : window.matchMedia("(prefers-reduced-motion: reduce)").matches;
}
