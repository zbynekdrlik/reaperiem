// IEM Mixer Service Worker — PWA shell only, no asset caching.
// Asset caching is handled by the browser's HTTP cache + Trunk content hashes.
// DO NOT add fetch caching here — it caused blank pages on deploy (2026-03-19).

self.addEventListener("install", () => {
  self.skipWaiting();
});

self.addEventListener("activate", (event) => {
  // Purge ALL caches from previous versions to fix stale asset issues
  event.waitUntil(
    caches
      .keys()
      .then((names) => Promise.all(names.map((name) => caches.delete(name))))
      .then(() => self.clients.claim()),
  );
});
