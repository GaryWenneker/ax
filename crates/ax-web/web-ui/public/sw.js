/* ax Command Center — network-first; never cache API or HTML shells */
const CACHE = "ax-shell-v4";

self.addEventListener("install", (event) => {
  event.waitUntil(self.skipWaiting());
});

self.addEventListener("activate", (event) => {
  event.waitUntil(
    caches.keys().then((keys) =>
      Promise.all(keys.map((k) => caches.delete(k))),
    ).then(() => self.clients.claim()),
  );
});

self.addEventListener("fetch", (event) => {
  const req = event.request;
  if (req.method !== "GET") return;

  const url = new URL(req.url);
  // Always hit the network for API + SSE — never respondWith.
  if (url.pathname === "/api" || url.pathname.startsWith("/api/")) return;

  // HTML navigations: network-only (no cache put) so Command Center never sticks on a stale shell.
  if (req.mode === "navigate" || url.pathname === "/" || url.pathname.endsWith(".html")) {
    event.respondWith(
      fetch(req).catch(() => caches.match("/").then((c) => c || Response.error())),
    );
    return;
  }

  // Hashed assets: network-first, cache only successful asset responses as offline fallback.
  if (url.pathname.startsWith("/assets/")) {
    event.respondWith(
      fetch(req)
        .then((res) => {
          if (res.ok) {
            const copy = res.clone();
            caches.open(CACHE).then((c) => c.put(req, copy)).catch(() => {});
          }
          return res;
        })
        .catch(() => caches.match(req).then((c) => c || Response.error())),
    );
    return;
  }

  // Everything else (icons, manifest): network-first with cache fallback.
  event.respondWith(
    fetch(req)
      .then((res) => {
        if (res.ok) {
          const copy = res.clone();
          caches.open(CACHE).then((c) => c.put(req, copy)).catch(() => {});
        }
        return res;
      })
      .catch(() => caches.match(req).then((c) => c || Response.error())),
  );
});
