// Runs before first paint so a saved light/dark choice never flashes the other theme.
(function () {
  try {
    var theme = localStorage.getItem('tt_theme');
    if (theme === 'light' || theme === 'dark') document.documentElement.setAttribute('data-theme', theme);
  } catch (e) {
    // Storage can be blocked; the system theme applies then.
  }
})();
