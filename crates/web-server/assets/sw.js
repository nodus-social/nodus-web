// The service worker the browser keeps subscribed to Web Push.
//
// The sender pushes a small JSON object: `{title, body, tag, data:{url}, icon}` and, for
// kinds where a replacement should still make a sound, `renotify: true`. Everything except
// `title`/`body` is optional, so this file prefers the sender's answer and falls back
// rather than inventing one — the sender knows whether two notifications are about the
// same thing; a service worker guessing from `kind` alone would collapse every mention on
// the instance into one banner.
//
// Plain JavaScript on purpose: it is served by the SSR host from `assets/` as one file, and
// nothing here is worth a bundler's build step.

/// The sender's tag when it names one, so two notifications about the SAME post replace
/// each other and two about different posts never do. `kind` is only a fallback for a
/// sender that predates the tag.
const tagFor = (payload) =>
  payload.tag || 'nodus-' + (payload.kind || 'notification');

/// Where a click lands: the sender's `data.url`, then a bare `url`, then the notifications
/// column. The server sends `data.url`; the others keep an older payload working.
const urlFor = (payload) =>
  (payload.data && payload.data.url) || payload.url || '/notifications';

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
  const options = {
    body,
    tag: tagFor(payload),
    // The sender decides whether a replacement should still alert: a second favourite on
    // the same post is news, a second notification about one follower is not.
    renotify: payload.renotify === true,
    data: { url: urlFor(payload) },
  };
  if (payload.icon) {
    // Only when the sender names one: an icon path that does not exist is a broken image
    // in a place nobody can debug.
    options.icon = payload.icon;
  }
  event.waitUntil(self.registration.showNotification(title, options));
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
