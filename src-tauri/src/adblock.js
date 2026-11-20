(() => {
  if (window.top !== window || window.__tweaxAdLoaded) return;
  window.__tweaxAdLoaded = true;
  const cfg = window.__TWEAX_CFG || {};
  const allowed = () => {
    const list = window.__tweaxAllow || cfg.allow || [];
    const h = location.hostname;
    return list.some((d) => h === d || h.endsWith("." + d));
  };
  const CSS_TEXT = [
    "ins.adsbygoogle,.adsbygoogle,[id^='google_ads'],[id^='div-gpt-ad'],[id^='aswift_'],[id^='google_ads_iframe']",
    "[id^='ad-slot'],[id^='ad_container'],[class~='advertisement'],[class*=' ad-container'],[class^='ad-container']",
    "[class*='advert-banner'],[class*='advertisement'],[data-ad],[data-ad-slot],[data-ad-unit],[data-ad-client],[data-google-query-id]",
    "[aria-label='advertisement' i],[aria-label='sponsored' i],amp-ad,amp-embed[type='taboola']",
    "iframe[src*='doubleclick'],iframe[src*='googlesyndication'],iframe[src*='adsystem'],iframe[id*='google_ads']",
    ".ad-banner,.ad-container,.ad-wrapper,.ad_unit,.ad-unit,.adunit,.ad-slot,.pb-ad,.ads-container,.sponsored-content",
    "#taboola-below-article,#taboola-right-rail,.taboola-placeholder,.OUTBRAIN,.ob-smartfeed,.mgbox,.revcontent",
    "ytd-ad-slot-renderer,ytd-display-ad-renderer,ytd-promoted-sparkles-web-renderer,ytd-promoted-video-renderer",
    "ytd-in-feed-ad-layout-renderer,ytd-banner-promo-renderer,ytd-companion-slot-renderer,ytd-action-companion-ad-renderer",
    "#player-ads,#masthead-ad,.video-ads,.ytp-ad-module,.ytp-ad-overlay-container,.ytp-ad-player-overlay",
    ".ytp-ad-image-overlay,.ytp-ad-text-overlay,.ytp-ad-overlay-slot",
  ].join(",") + "{display:none!important}";

  let sheet = null;
  let fallback = null;
  const attach = () => {
    if (sheet === null && fallback === null) {
      try {
        sheet = new CSSStyleSheet();
        sheet.replaceSync(CSS_TEXT);
      } catch (_) {
        sheet = false;
      }
    }
    if (sheet) {
      if (!document.adoptedStyleSheets.includes(sheet)) document.adoptedStyleSheets = [...document.adoptedStyleSheets, sheet];
      return;
    }
    if (!fallback) {
      fallback = document.createElement("style");
      fallback.textContent = CSS_TEXT;
    }
    const parent = document.head || document.documentElement;
    if (parent && !fallback.isConnected) parent.appendChild(fallback);
  };
  const detach = () => {
    if (sheet) document.adoptedStyleSheets = document.adoptedStyleSheets.filter((s) => s !== sheet);
    if (fallback) fallback.remove();
  };
  const sync = () => { if (allowed()) detach(); else attach(); };
  window.__tweaxAdSync = sync;
  sync();
  document.addEventListener("DOMContentLoaded", sync);

  if (/(^|\.)youtube(-nocookie)?\.com$/i.test(location.hostname)) {
    const SKIP = ".ytp-ad-skip-button,.ytp-ad-skip-button-modern,button.ytp-skip-ad-button,.ytp-ad-skip-button-container button";
    let queued = false;
    const run = () => {
      queued = false;
      if (allowed()) return;
      const b = document.querySelector(SKIP);
      if (b && b.getClientRects().length) { try { b.click(); } catch (_) {} }
    };
    const queue = () => {
      if (queued) return;
      queued = true;
      requestAnimationFrame(run);
    };
    new MutationObserver(queue).observe(document, { childList: true, subtree: true });
    document.addEventListener("yt-navigate-finish", queue, true);
    queue();
  }
})();
