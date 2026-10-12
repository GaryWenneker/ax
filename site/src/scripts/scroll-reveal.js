import { motionPaused } from "./motion.js";

// Progressive enhancement: no script/observer means all content stays visible.
export function mountScrollReveal() {
  if (!("IntersectionObserver" in window)) return () => {};
  const items = document.querySelectorAll([
    ".hero-copy > *:not(.hero-install)", ".hero-install", ".ax-live-console",
    ".capability", ".story-intro > *", ".stream-head", ".stream-cluster",
    ".workflow-row", ".product-figure", ".deployment-rows > a",
    ".final-section > div:first-child > *", ".final-section > .install-block",
    ".signal-footer > *", ".sl-markdown-content > *", "h1#_top",
  ].join(","));
  const observer = new IntersectionObserver(entries => {
    for (const entry of entries) {
      if (entry.isIntersecting) show(entry.target);
    }
  }, { threshold: 0, rootMargin: "0px 0px -24px 0px" });
  function show(item) {
    item.classList.remove("ax-reveal-pending");
    observer.unobserve(item);
  }
  function exposeAll() {
    if (motionPaused()) items.forEach(show);
  }
  for (const item of items) {
    item.classList.add("ax-reveal");
    if (!motionPaused()) {
      item.classList.add("ax-reveal-pending");
      observer.observe(item);
    }
  }
  // Keyboard navigation must never land on hidden content.
  const focus = event => items.forEach(item => { if (item.contains(event.target)) show(item); });
  document.addEventListener("focusin", focus);
  window.addEventListener("ax:motion", exposeAll);
  const preference = window.matchMedia("(prefers-reduced-motion: reduce)");
  preference.addEventListener("change", exposeAll);
  return () => {
    observer.disconnect();
    items.forEach(show);
    document.removeEventListener("focusin", focus);
    window.removeEventListener("ax:motion", exposeAll);
    preference.removeEventListener("change", exposeAll);
  };
}
