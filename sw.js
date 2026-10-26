// Caches the app shell so the installed app opens offline.
const CACHE = "easy-ref";

self.addEventListener("install", () => self.skipWaiting());
self.addEventListener("activate", (event) => event.waitUntil(self.clients.claim()));

self.addEventListener("fetch", (event) => {
  const { request } = event;
  if (request.method !== "GET" || new URL(request.url).origin !== self.location.origin) return;
  event.respondWith(request.mode === "navigate" ? networkFirst(request) : cacheFirst(request));
});

async function networkFirst(request) {
  const cache = await caches.open(CACHE);
  try {
    const response = await fetch(request);
    if (response.ok) {
      await cache.put(self.registration.scope, response.clone());
      await prune(cache, await response.clone().text());
    }
    return response;
  } catch {
    return (await cache.match(self.registration.scope)) || Response.error();
  }
}

async function cacheFirst(request) {
  const cache = await caches.open(CACHE);
  const cached = await cache.match(request);
  if (cached) return cached;
  const response = await fetch(request);
  if (response.ok) await cache.put(request, response.clone());
  return response;
}

// Removes build files that the current index.html no longer references.
async function prune(cache, html) {
  for (const key of await cache.keys()) {
    const name = new URL(key.url).pathname.split("/").pop();
    if (key.url !== self.registration.scope && !html.includes(name)) await cache.delete(key);
  }
}
