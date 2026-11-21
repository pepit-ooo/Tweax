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
    const AD_KEYS = ["adPlacements", "playerAds", "adSlots", "adBreakHeartbeatParams"];
    const prune = (o) => {
      if (!o || typeof o !== "object") return o;
      for (const k of AD_KEYS) if (k in o) { try { delete o[k]; } catch (_) {} }
      const pr = o.playerResponse;
      if (pr && typeof pr === "object") for (const k of AD_KEYS) if (k in pr) { try { delete pr[k]; } catch (_) {} }
      return o;
    };

    try {
      const parse = JSON.parse;
      JSON.parse = function () {
        const r = parse.apply(this, arguments);
        try { if (!allowed()) prune(r); } catch (_) {}
        return r;
      };
    } catch (_) {}

    try {
      const rjson = Response.prototype.json;
      Response.prototype.json = function () {
        const url = this.url || "";
        return rjson.call(this).then((d) => {
          try { if (/\/youtubei\/v1\/(player|next|browse|reel)/.test(url) && !allowed()) prune(d); } catch (_) {}
          return d;
        });
      };
    } catch (_) {}

    try {
      let initial;
      Object.defineProperty(window, "ytInitialPlayerResponse", {
        configurable: true,
        get() { return initial; },
        set(v) { try { if (!allowed()) prune(v); } catch (_) {} initial = v; },
      });
    } catch (_) {}

    const SKIP = ".ytp-ad-skip-button,.ytp-ad-skip-button-modern,button.ytp-skip-ad-button,.ytp-ad-skip-button-container button";
    const EXTRA_CSS = "ytd-rich-item-renderer:has(ytd-ad-slot-renderer),ytd-enforcement-message-view-model,tp-yt-iron-overlay-backdrop.opened{display:none!important}";
    try {
      const extra = new CSSStyleSheet();
      extra.replaceSync(EXTRA_CSS);
      const addExtra = () => {
        if (allowed()) document.adoptedStyleSheets = document.adoptedStyleSheets.filter((x) => x !== extra);
        else if (!document.adoptedStyleSheets.includes(extra)) document.adoptedStyleSheets = [...document.adoptedStyleSheets, extra];
      };
      const prevSync = window.__tweaxAdSync;
      window.__tweaxAdSync = () => { prevSync(); addExtra(); };
      addExtra();
    } catch (_) {}

    let saved = null;
    const run = () => {
      queued = false;
      if (allowed()) return;
      const player = document.querySelector(".html5-video-player");
      const video = document.querySelector("video.html5-main-video") || (player && player.querySelector("video"));
      const ad = !!(player && player.classList.contains("ad-showing"));
      if (ad && video) {
        if (!saved) saved = { rate: video.playbackRate, muted: video.muted };
        try {
          video.muted = true;
          video.playbackRate = 16;
          if (isFinite(video.duration) && video.duration > 0 && video.currentTime < video.duration - 0.1) video.currentTime = video.duration;
        } catch (_) {}
      } else if (saved && video) {
        try { video.playbackRate = saved.rate || 1; video.muted = saved.muted; } catch (_) {}
        saved = null;
      }
      const b = document.querySelector(SKIP);
      if (b && b.getClientRects().length) { try { b.click(); } catch (_) {} }
      const warn = document.querySelector("ytd-enforcement-message-view-model");
      if (warn) {
        warn.remove();
        document.querySelectorAll("tp-yt-iron-overlay-backdrop").forEach((e) => e.remove());
        try { document.body.style.overflow = ""; document.documentElement.style.overflow = ""; } catch (_) {}
        if (video && video.paused) { try { video.play(); } catch (_) {} }
      }
    };
    let queued = false;
    const queue = () => {
      if (queued) return;
      queued = true;
      requestAnimationFrame(run);
    };
    new MutationObserver(queue).observe(document, { childList: true, subtree: true, attributes: true, attributeFilter: ["class"] });
    document.addEventListener("yt-navigate-finish", queue, true);
    setInterval(queue, 500);
    queue();
  }
})();
