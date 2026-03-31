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

// Handle alert notifications from WASM (works when app is in background)
self.addEventListener("message", (event) => {
  if (event.data && event.data.type === "ALERT") {
    const name = event.data.name || "Member";
    self.registration.showNotification(`IEM Alert: ${name}`, {
      body: `${name} needs help!`,
      requireInteraction: true,
      tag: "iem-alert", // Replace previous alert notification
      vibrate: [500, 200, 500, 200, 500],
    });
  }
});

// Clicking notification brings app to foreground
self.addEventListener("notificationclick", (event) => {
  event.notification.close();
  event.waitUntil(
    self.clients.matchAll({ type: "window" }).then((clients) => {
      if (clients.length > 0) {
        return clients[0].focus();
      }
      return self.clients.openWindow("/engineer");
    }),
  );
});

// Handle Web Push notifications (works even when app is fully closed) (#133)
self.addEventListener("push", (event) => {
  let data = {};
  try {
    data = event.data?.json() ?? {};
  } catch {
    data = {};
  }

  if (data.type === "SOS") {
    event.waitUntil(
      self.registration.showNotification(`IEM Alert: ${data.name || "Member"}`, {
        body: `${data.name || "Someone"} needs help!`,
        requireInteraction: true,
        tag: "iem-alert",
        vibrate: [500, 200, 500, 200, 500],
      }),
    );
  } else {
    // Generic fallback for unknown push types
    event.waitUntil(
      self.registration.showNotification("IEM Mixer", {
        body: "New alert — tap to open",
        requireInteraction: true,
        tag: "iem-generic",
      }),
    );
  }
});
