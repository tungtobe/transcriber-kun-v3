// This same-origin, dependency-free bootstrap runs before Vite's production
// stylesheet and prevents the cached/system theme from flashing light first.
(function applyEarlyTheme() {
  var preference = 'system';
  try {
    var cached = window.localStorage.getItem('trans-kun.theme');
    if (cached === 'light' || cached === 'dark' || cached === 'system') {
      preference = cached;
    }
  } catch (_error) {
    // System theme remains a safe first-paint fallback when storage is blocked.
  }

  var systemDark = false;
  try {
    systemDark = Boolean(
      window.matchMedia && window.matchMedia('(prefers-color-scheme: dark)').matches,
    );
  } catch (_error) {
    systemDark = false;
  }

  var resolved = preference === 'dark' || (preference === 'system' && systemDark)
    ? 'dark'
    : 'light';
  document.documentElement.dataset.theme = resolved;
  document.documentElement.dataset.themePreference = preference;
})();
