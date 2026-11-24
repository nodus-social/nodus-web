// The service worker the browser keeps subscribed to Web Push.
//
// The sender (`crates/api/src/webpush.rs`) pushes a small JSON object: `{title, body}`.
// That is all it sends, so this file must not expect more: a payload that already names a
// `kind` tags the notification per kind, and every payload without one shares a single tag.
// Either way five favourites replace each other instead of stacking five banners — a civil
// instance produces more notifications than a lock screen can hold.
//
// Plain JavaScript on purpose: it is served by the SSR host from `assets/` as one file, and
// nothing here is worth a bundler's build step.

/// One tag per kind, so a burst of one kind collapses to one banner while a follow is not
/// replaced by a favourite.
const tagFor = (payload) => 'nodus-' + (payload.kind || 'notification');

/// Where a click lands: the notification's own URL when the sender names one, else the
/// notifications column.
const urlFor = (payload) => payload.url || '/notifications';

self.addEventListener('push', (event) => {
  if (!event.data) {
    return;
  }
  let payload;
  try {
    payload = event.data.json();
  } catch {
    return;
  }

  const title = payload.title || 'Nodus';
  const body = payload.body || 'You have a new notification';
  event.waitUntil(
    self.registration.showNotification(title, {
      body,
      tag: tagFor(payload),
      renotify: true,
      data: { url: urlFor(payload) },
    }),
  );
});

self.addEventListener('notificationclick', (event) => {
  event.notification.close();
  const target = new URL(
    (event.notification.data && event.notification.data.url) || '/notifications',
    self.registration.scope,
  );

  event.waitUntil(
    (async () => {
      const windows = await self.clients.matchAll({
        type: 'window',
        includeUncontrolled: true,
      });
      // A tab already showing the notification's page is brought forward; opening a second
      // copy of the same column would leave the reader with two identical pages.
      for (const client of windows) {
        if (new URL(client.url).href === target.href && 'focus' in client) {
          return client.focus();
        }
      }
      return self.clients.openWindow(target.href);
    })(),
  );
});
