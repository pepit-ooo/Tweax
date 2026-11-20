(() => {
  if (window.top !== window || window.__tweaxLoaded) return;
  window.__tweaxLoaded = true;
  const cfg = window.__TWEAX_CFG || {};
  const norm = (e) => {
    const k = e.key;
    if (!k || k === "Control" || k === "Shift" || k === "Alt" || k === "Meta") return "";
    const parts = [];
    if (e.ctrlKey || e.metaKey) parts.push("ctrl");
    if (e.altKey) parts.push("alt");
    if (e.shiftKey) parts.push("shift");
    parts.push(k.toLowerCase());
    return parts.join("+");
  };
  const sendKey = (combo) => {
    const prev = document.title;
    document.title = "​tweax-key:" + combo + ":" + Date.now();
    setTimeout(() => { document.title = prev; }, 0);
  };

  const host = document.createElement("div");
  host.style.cssText = "all:initial;position:fixed;top:0;left:0;width:0;height:0;z-index:2147483647;";
  const root = host.attachShadow({ mode: "closed" });
  root.innerHTML = `
    <style>
      *{box-sizing:border-box;font-family:"Segoe UI Variable Text","Segoe UI",system-ui,sans-serif}
      .find{position:fixed;top:14px;right:18px;display:none;align-items:center;gap:6px;padding:7px 8px 7px 14px;border-radius:14px;background:rgba(24,24,28,.72);border:1px solid rgba(255,255,255,.16);backdrop-filter:blur(24px) saturate(1.5);-webkit-backdrop-filter:blur(24px) saturate(1.5);box-shadow:0 12px 40px rgba(0,0,0,.35);color:#fff;font-size:13px}
      .find.on{display:flex}
      .find input{width:190px;height:28px;border:0;outline:0;background:transparent;color:#fff;font-size:13.5px}
      .find input::placeholder{color:rgba(255,255,255,.5)}
      .cnt{min-width:46px;text-align:right;color:rgba(255,255,255,.55);font-size:12px;font-variant-numeric:tabular-nums}
      .find button{width:28px;height:28px;border:0;border-radius:8px;background:transparent;color:#fff;cursor:pointer;display:grid;place-items:center;font-size:15px;line-height:1}
      .find button:hover{background:rgba(255,255,255,.14)}
      .toast{position:fixed;left:50%;bottom:28px;transform:translateX(-50%);padding:8px 16px;border-radius:99px;background:rgba(24,24,28,.72);border:1px solid rgba(255,255,255,.16);backdrop-filter:blur(24px);-webkit-backdrop-filter:blur(24px);color:#fff;font-size:13px;opacity:0;transition:opacity .2s;pointer-events:none}
      .toast.on{opacity:1}
      .rd{position:fixed;inset:0;display:none;overflow:auto;background:var(--bg);color:var(--fg);font-family:Georgia,"Times New Roman",serif}
      .rd.on{display:block}
      .rd[data-t="clair"]{--bg:#fafaf7;--fg:#222;--mu:#6b6b66;--ln:rgba(0,0,0,.14);--lk:#1a5fd0}
      .rd[data-t="sepia"]{--bg:#f3ead7;--fg:#3b3226;--mu:#7d6f5a;--ln:rgba(60,45,20,.18);--lk:#8a4b12}
      .rd[data-t="sombre"]{--bg:#16171a;--fg:#d8d8d4;--mu:#8d8d88;--ln:rgba(255,255,255,.16);--lk:#7fb0ff}
      .rdbar{position:sticky;top:0;display:flex;align-items:center;gap:6px;padding:10px 18px;background:var(--bg);border-bottom:1px solid var(--ln);font-family:"Segoe UI",system-ui,sans-serif;font-size:13px;color:var(--mu);z-index:1}
      .rdbar .sp{flex:1}
      .rdbar button{height:30px;min-width:34px;padding:0 10px;border:1px solid var(--ln);border-radius:9px;background:transparent;color:var(--fg);cursor:pointer;font-size:13px}
      .rdbar button:hover{background:var(--ln)}
      .rdwrap{max-width:700px;margin:0 auto;padding:36px 22px 90px;font-size:var(--fs,19px);line-height:1.75}
      .rdwrap h1{font-size:1.75em;line-height:1.25;margin:0 0 .3em}
      .rdwrap h2,.rdwrap h3,.rdwrap h4{line-height:1.3;margin:1.6em 0 .5em}
      .rdwrap .meta{font-family:"Segoe UI",system-ui,sans-serif;font-size:13px;color:var(--mu);margin-bottom:2em}
      .rdwrap p{margin:0 0 1.1em}
      .rdwrap a{color:var(--lk)}
      .rdwrap img{max-width:100%;height:auto;display:block;margin:1.4em auto;border-radius:6px}
      .rdwrap figure{margin:1.4em 0}
      .rdwrap figcaption{font-size:.8em;color:var(--mu);text-align:center}
      .rdwrap blockquote{margin:1.2em 0;padding:.1em 0 .1em 1.1em;border-left:3px solid var(--ln);color:var(--mu)}
      .rdwrap pre{overflow:auto;padding:12px 14px;border-radius:8px;border:1px solid var(--ln);font:13px/1.5 Consolas,monospace}
      .rdwrap code{font:.88em Consolas,monospace}
      .rdwrap table{border-collapse:collapse;max-width:100%;display:block;overflow:auto}
      .rdwrap td,.rdwrap th{border:1px solid var(--ln);padding:6px 10px}
      .rdwrap hr{border:0;border-top:1px solid var(--ln);margin:2em 0}
    </style>
    <div class="find" id="find">
      <input id="q" placeholder="Rechercher dans la page" spellcheck="false" autocomplete="off">
      <span class="cnt" id="cnt"></span>
      <button id="prev" title="Précédent">&#8593;</button>
      <button id="next" title="Suivant">&#8595;</button>
      <button id="close" title="Fermer">&#10005;</button>
    </div>
    <div class="toast" id="toast"></div>
    <div class="rd" id="rd" data-t="clair">
      <div class="rdbar">
        <span id="rdinfo"></span><span class="sp"></span>
        <button id="rdm" title="Texte plus petit">A−</button>
        <button id="rdp" title="Texte plus grand">A+</button>
        <button id="rdt" title="Changer de thème">Thème</button>
        <button id="rdx" title="Fermer">✕</button>
      </div>
      <div class="rdwrap" id="rdbody"></div>
    </div>`;
  const $ = (id) => root.getElementById(id);
  const mount = () => { if (!host.isConnected) (document.documentElement || document).appendChild(host); };

  const hlStyle = document.createElement("style");
  hlStyle.textContent = "::highlight(tweax-find){background:#ffe066;color:#000}::highlight(tweax-find-cur){background:#ff9f0a;color:#000}";
  let ranges = [];
  let idx = 0;
  const hasHl = typeof CSS !== "undefined" && CSS.highlights && typeof Highlight !== "undefined";

  const clearHl = () => {
    ranges = [];
    idx = 0;
    if (hasHl) { CSS.highlights.delete("tweax-find"); CSS.highlights.delete("tweax-find-cur"); }
    $("cnt").textContent = "";
  };
  const showCur = () => {
    if (!ranges.length) { $("cnt").textContent = $("q").value ? "0/0" : ""; return; }
    const r = ranges[idx];
    if (hasHl) CSS.highlights.set("tweax-find-cur", new Highlight(r));
    const el = r.startContainer.parentElement;
    if (el && el.scrollIntoView) el.scrollIntoView({ block: "center", inline: "nearest" });
    $("cnt").textContent = (idx + 1) + "/" + ranges.length;
  };
  const search = (q) => {
    clearHl();
    if (!q || !hasHl) return;
    const lq = q.toLowerCase();
    const walker = document.createTreeWalker(document.body || document.documentElement, NodeFilter.SHOW_TEXT, {
      acceptNode(n) {
        const p = n.parentElement;
        if (!p) return NodeFilter.FILTER_REJECT;
        const t = p.tagName;
        if (t === "SCRIPT" || t === "STYLE" || t === "NOSCRIPT" || t === "TEXTAREA") return NodeFilter.FILTER_REJECT;
        if (host.contains(n)) return NodeFilter.FILTER_REJECT;
        return NodeFilter.FILTER_ACCEPT;
      },
    });
    let node;
    while ((node = walker.nextNode()) && ranges.length < 3000) {
      const txt = node.nodeValue.toLowerCase();
      let i = 0;
      while ((i = txt.indexOf(lq, i)) >= 0) {
        const r = new Range();
        r.setStart(node, i);
        r.setEnd(node, i + q.length);
        ranges.push(r);
        i += q.length;
      }
    }
    if (ranges.length) CSS.highlights.set("tweax-find", new Highlight(...ranges));
    showCur();
  };
  const step = (d) => {
    if (!ranges.length) return;
    idx = (idx + d + ranges.length) % ranges.length;
    showCur();
  };
  const openFind = () => {
    mount();
    if (!hlStyle.isConnected) (document.head || document.documentElement).appendChild(hlStyle);
    $("find").classList.add("on");
    const q = $("q");
    const sel = String(getSelection() || "").trim();
    if (sel && sel.length < 80) q.value = sel;
    q.focus();
    q.select();
    search(q.value);
  };
  const closeFind = () => {
    $("find").classList.remove("on");
    clearHl();
  };
  $("q").addEventListener("input", (e) => search(e.target.value));
  $("q").addEventListener("keydown", (e) => {
    if (e.key === "Enter") { e.preventDefault(); step(e.shiftKey ? -1 : 1); }
    if (e.key === "Escape") { e.preventDefault(); closeFind(); }
    e.stopPropagation();
  });
  $("prev").onclick = () => step(-1);
  $("next").onclick = () => step(1);
  $("close").onclick = closeFind;
  window.__tweaxFind = { open: openFind, close: closeFind };

  let toastTimer = 0;
  const toast = (t) => {
    mount();
    const el = $("toast");
    el.textContent = t;
    el.classList.add("on");
    clearTimeout(toastTimer);
    toastTimer = setTimeout(() => el.classList.remove("on"), 1100);
  };

  const RD_KEY = "__tweax_reader";
  const RD_THEMES = ["clair", "sepia", "sombre"];
  const rdPrefs = { size: 19, theme: "clair" };
  try { Object.assign(rdPrefs, JSON.parse(localStorage.getItem(RD_KEY) || "{}")); } catch (_) {}
  const rdSave = () => { try { localStorage.setItem(RD_KEY, JSON.stringify(rdPrefs)); } catch (_) {} };
  const rdApply = () => {
    if (!RD_THEMES.includes(rdPrefs.theme)) rdPrefs.theme = "clair";
    rdPrefs.size = Math.min(32, Math.max(13, Number(rdPrefs.size) || 19));
    $("rd").dataset.t = rdPrefs.theme;
    $("rd").style.setProperty("--fs", rdPrefs.size + "px");
  };
  const RD_KEEP = new Set(["P", "H1", "H2", "H3", "H4", "UL", "OL", "LI", "BLOCKQUOTE", "PRE", "CODE", "FIGURE", "FIGCAPTION", "IMG", "A", "STRONG", "EM", "B", "I", "BR", "TABLE", "THEAD", "TBODY", "TR", "TD", "TH", "SUB", "SUP", "HR"]);
  const RD_DROP = new Set(["SCRIPT", "STYLE", "NOSCRIPT", "NAV", "ASIDE", "FOOTER", "FORM", "BUTTON", "IFRAME", "SVG", "INPUT", "SELECT", "TEXTAREA", "VIDEO", "AUDIO", "CANVAS", "DIALOG", "TEMPLATE", "OBJECT", "EMBED"]);
  const RD_NOISE = /comment|share|social|related|promo|newsletter|advert|sidebar|cookie|popup|modal|subscribe|breadcrumb|toolbar/i;
  const rdOk = (u) => /^https?:\/\//i.test(u);

  const rdScore = (el, cache) => {
    if (cache.has(el)) return cache.get(el);
    let s = 0;
    el.querySelectorAll("p").forEach((p) => {
      const t = p.textContent.trim().length;
      if (t < 50) return;
      let links = 0;
      p.querySelectorAll("a").forEach((a) => { links += a.textContent.length; });
      s += Math.max(0, t - links * 1.5);
    });
    cache.set(el, s);
    return s;
  };
  const rdPickRoot = () => {
    const cache = new Map();
    const total = rdScore(document.body, cache);
    if (total < 200) return null;
    const art = Array.from(document.querySelectorAll("article, [role=main], main")).sort((a, b) => rdScore(b, cache) - rdScore(a, cache))[0];
    let cur = art && rdScore(art, cache) >= total * 0.5 ? art : document.body;
    for (let guard = 0; guard < 30; guard++) {
      const here = rdScore(cur, cache);
      let next = null;
      for (const c of cur.children) { if (rdScore(c, cache) >= here * 0.6) { next = c; break; } }
      if (!next) break;
      cur = next;
    }
    return cur;
  };
  const rdBuild = (src, dst) => {
    for (const n of Array.from(src.childNodes)) {
      if (n.nodeType === 3) { dst.appendChild(document.createTextNode(n.nodeValue)); continue; }
      if (n.nodeType !== 1) continue;
      const tag = n.tagName.toUpperCase();
      if (RD_DROP.has(tag) || tag === "HEADER" && !n.querySelector("h1,h2")) continue;
      if (RD_NOISE.test((n.id || "") + " " + (typeof n.className === "string" ? n.className : ""))) continue;
      let cs = null;
      try { cs = getComputedStyle(n); } catch (_) {}
      if (cs && (cs.display === "none" || cs.visibility === "hidden")) continue;
      if (!RD_KEEP.has(tag)) { rdBuild(n, dst); continue; }
      const el = document.createElement(tag.toLowerCase());
      if (tag === "A") {
        const h = n.getAttribute("href") || "";
        if (rdOk(n.href)) { el.href = n.href; el.target = "_blank"; el.rel = "noopener noreferrer"; }
        else if (!h) continue;
      }
      if (tag === "IMG") {
        const u = n.currentSrc || n.src || "";
        if (!rdOk(u) || (n.naturalWidth && n.naturalWidth < 120)) continue;
        el.src = u;
        el.alt = n.alt || "";
      } else if (tag !== "BR" && tag !== "HR") rdBuild(n, el);
      dst.appendChild(el);
    }
  };
  const rdOpen = () => {
    const rootEl = rdPickRoot();
    if (!rootEl) return toast("Page non lisible");
    const body = $("rdbody");
    body.textContent = "";
    const h1 = (rootEl.querySelector("h1") || document.querySelector("h1"));
    const title = (h1 && h1.textContent.trim()) || document.title || location.hostname;
    const head = document.createElement("h1");
    head.textContent = title;
    const meta = document.createElement("div");
    meta.className = "meta";
    const content = document.createElement("div");
    rdBuild(rootEl, content);
    content.querySelectorAll("h1").forEach((x) => { if (x.textContent.trim() === title) x.remove(); });
    content.querySelectorAll("p,li,figure,blockquote,h2,h3,h4").forEach((x) => { if (!x.textContent.trim() && !x.querySelector("img")) x.remove(); });
    const words = content.textContent.trim().split(/\s+/).filter(Boolean).length;
    if (words < 60) return toast("Page non lisible");
    meta.textContent = location.hostname + " · " + Math.max(1, Math.round(words / 220)) + " min de lecture";
    body.append(head, meta, content);
    rdApply();
    mount();
    $("rd").classList.add("on");
    $("rd").scrollTop = 0;
    document.documentElement.dataset.tweaxRd = document.documentElement.style.overflow || "";
    document.documentElement.style.overflow = "hidden";
  };
  const rdClose = () => {
    $("rd").classList.remove("on");
    $("rdbody").textContent = "";
    const prev = document.documentElement.dataset.tweaxRd;
    document.documentElement.style.overflow = prev || "";
    delete document.documentElement.dataset.tweaxRd;
  };
  const rdIsOpen = () => $("rd").classList.contains("on");
  $("rdx").onclick = rdClose;
  $("rdm").onclick = () => { rdPrefs.size -= 1; rdApply(); rdSave(); };
  $("rdp").onclick = () => { rdPrefs.size += 1; rdApply(); rdSave(); };
  $("rdt").onclick = () => { rdPrefs.theme = RD_THEMES[(RD_THEMES.indexOf(rdPrefs.theme) + 1) % RD_THEMES.length]; rdApply(); rdSave(); };
  window.__tweaxReader = { toggle: () => (rdIsOpen() ? rdClose() : rdOpen()) };

  const ZKEY = "__tweax_zoom";
  let zoom = 1;
  try { zoom = parseFloat(localStorage.getItem(ZKEY)) || 1; } catch (_) {}
  const applyZoom = () => {
    const el = document.documentElement;
    if (el) el.style.zoom = zoom === 1 ? "" : String(zoom);
  };
  const setZoom = (z) => {
    zoom = Math.min(5, Math.max(0.3, Math.round(z * 100) / 100));
    try { localStorage.setItem(ZKEY, String(zoom)); } catch (_) {}
    applyZoom();
    toast(Math.round(zoom * 100) + " %");
  };
  applyZoom();
  document.addEventListener("readystatechange", applyZoom);
  document.addEventListener("DOMContentLoaded", applyZoom);
  window.addEventListener("wheel", (e) => {
    if (!e.ctrlKey) return;
    e.preventDefault();
    setZoom(zoom + (e.deltaY < 0 ? 0.1 : -0.1));
  }, { passive: false, capture: true });

  window.addEventListener("keydown", (e) => {
    if (e.ctrlKey || e.metaKey) {
      if (e.key === "+" || e.key === "=") { e.preventDefault(); return setZoom(zoom + 0.1); }
      if (e.key === "-" || e.key === "_") { e.preventDefault(); return setZoom(zoom - 0.1); }
      if (e.key === "0") { e.preventDefault(); return setZoom(1); }
    }
    if (e.key === "Escape" && $("find").classList.contains("on")) { closeFind(); return; }
    if (e.key === "Escape" && rdIsOpen()) { e.preventDefault(); rdClose(); return; }
    const combo = norm(e);
    if (!combo) return;
    const keys = window.__tweaxKeys || cfg.keys || [];
    if (keys.includes(combo)) {
      e.preventDefault();
      e.stopPropagation();
      sendKey(combo);
    }
  }, true);

  if (cfg.adblock) {
    const BLOCK = ["doubleclick.net", "googlesyndication.com", "googleadservices.com", "google-analytics.com", "adservice.google.com", "adnxs.com", "adsrvr.org", "taboola.com", "outbrain.com", "criteo.com", "criteo.net", "rubiconproject.com", "pubmatic.com", "openx.net", "casalemedia.com", "amazon-adsystem.com", "scorecardresearch.com", "quantserve.com", "moatads.com", "adsafeprotected.com", "advertising.com", "2mdn.net", "yieldmo.com", "smartadserver.com", "teads.tv", "media.net", "bidswitch.net", "contextweb.com", "lijit.com", "sharethrough.com", "indexww.com", "zedo.com", "serving-sys.com", "hotjar.com", "mouseflow.com", "krxd.net", "demdex.net", "bluekai.com", "adform.net", "mathtag.com", "rlcdn.com", "tapad.com", "33across.com", "sitescout.com", "exelator.com", "everesttech.net", "adcolony.com", "adroll.com", "popads.net", "propellerads.com", "exoclick.com", "juicyads.com", "trafficjunky.net", "revcontent.com", "mgid.com", "ads.yahoo.com", "adtechus.com", "pagead2.googlesyndication.com", "fundingchoicesmessages.google.com"];
    const allowed = () => {
      const list = window.__tweaxAllow || cfg.allow || [];
      const h = location.hostname;
      return list.some((d) => h === d || h.endsWith("." + d));
    };
    const isBlocked = (u) => {
      if (allowed()) return false;
      try {
        const h = new URL(u, location.href).hostname;
        return BLOCK.some((d) => h === d || h.endsWith("." + d));
      } catch (_) { return false; }
    };
    const origFetch = window.fetch;
    if (origFetch) window.fetch = function (input, init) {
      const u = typeof input === "string" ? input : input && input.url;
      if (u && isBlocked(u)) return Promise.reject(new TypeError("blocked"));
      return origFetch.apply(this, arguments);
    };
    const xo = XMLHttpRequest.prototype.open;
    const xs = XMLHttpRequest.prototype.send;
    XMLHttpRequest.prototype.open = function (m, u) {
      this.__tw = isBlocked(String(u));
      return xo.apply(this, arguments);
    };
    XMLHttpRequest.prototype.send = function () {
      if (this.__tw) { try { this.abort(); } catch (_) {} return; }
      return xs.apply(this, arguments);
    };
    if (navigator.sendBeacon) {
      const sb = navigator.sendBeacon.bind(navigator);
      navigator.sendBeacon = (u, d) => (isBlocked(String(u)) ? true : sb(u, d));
    }
    for (const C of [HTMLScriptElement, HTMLImageElement, HTMLIFrameElement]) {
      const d = Object.getOwnPropertyDescriptor(C.prototype, "src");
      if (d && d.set) Object.defineProperty(C.prototype, "src", {
        get: d.get,
        set(v) { if (isBlocked(String(v))) return; d.set.call(this, v); },
        configurable: true,
      });
    }
    const sa = Element.prototype.setAttribute;
    Element.prototype.setAttribute = function (n, v) {
      if ((n === "src" || n === "href") && /^(SCRIPT|IMG|IFRAME|LINK)$/.test(this.tagName) && isBlocked(String(v))) return;
      return sa.apply(this, arguments);
    };
    const sweep = (n) => {
      if (n.nodeType !== 1) return;
      const list = n.matches && n.matches("script[src],iframe[src],img[src]") ? [n] : [];
      if (n.querySelectorAll) list.push(...n.querySelectorAll("script[src],iframe[src],img[src]"));
      for (const el of list) if (isBlocked(el.src)) el.remove();
    };
    new MutationObserver((ms) => { for (const m of ms) m.addedNodes.forEach(sweep); })
      .observe(document, { childList: true, subtree: true });
    const css = document.createElement("style");
    css.textContent = "ins.adsbygoogle,.adsbygoogle,[id^='google_ads'],[id^='div-gpt-ad'],iframe[src*='doubleclick.net'],iframe[src*='googlesyndication.com'],.ad-banner,.ad-container,.advert,.ads-container,[aria-label='Advertisement'],[data-ad-slot]{display:none!important}";
    const addCss = () => { if (!allowed()) (document.head || document.documentElement).appendChild(css); };
    if (document.documentElement) addCss(); else document.addEventListener("DOMContentLoaded", addCss);
  }
})();
