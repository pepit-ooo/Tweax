(() => {
  const PROFILE_KEYS = ["bookmarks", "history", "session", "sidebarApps", "downloads", "home", "wxcache"];
  const raw = {
    get(k, d) { try { const v = localStorage.getItem(k); return v ? JSON.parse(v) : d; } catch { return d; } },
    set(k, v) { try { localStorage.setItem(k, JSON.stringify(v)); } catch {} },
    del(k) { try { localStorage.removeItem(k); } catch {} },
  };

  const validId = (id) => typeof id === "string" && /^[A-Za-z0-9_-]{1,40}$/.test(id);
  const activeProfile = () => { const p = raw.get("profile", "default"); return validId(p) ? p : "default"; };
  const keyOf = (k) => (PROFILE_KEYS.includes(k) && activeProfile() !== "default" ? "p." + activeProfile() + "." + k : k);

  const store = {
    get(k, d) { return raw.get(keyOf(k), d); },
    set(k, v) { raw.set(keyOf(k), v); },
    del(k) { raw.del(keyOf(k)); },
  };

  const profiles = () => {
    const l = raw.get("profiles", []);
    const extra = Array.isArray(l) ? l.filter((p) => p && validId(p.id) && p.id !== "default" && typeof p.name === "string") : [];
    return [{ id: "default", name: "Principal" }].concat(extra);
  };
  const saveProfiles = (list) => raw.set("profiles", list.filter((p) => p.id !== "default"));
  const profileName = () => (profiles().find((p) => p.id === activeProfile()) || { name: "Principal" }).name;
  const setProfile = (id) => raw.set("profile", validId(id) ? id : "default");
  const newProfileId = () => "p" + Date.now().toString(36);
  const dropProfileData = (id) => {
    if (!validId(id) || id === "default") return;
    const pre = "p." + id + ".";
    const del = [];
    for (let i = 0; i < localStorage.length; i++) { const k = localStorage.key(i); if (k && k.startsWith(pre)) del.push(k); }
    del.forEach((k) => raw.del(k));
  };

  const ENGINES = {
    google:     { name: "Google",     url: "https://www.google.com/search?q=%s" },
    duckduckgo: { name: "DuckDuckGo", url: "https://duckduckgo.com/?q=%s" },
    bing:       { name: "Bing",       url: "https://www.bing.com/search?q=%s" },
    brave:      { name: "Brave",      url: "https://search.brave.com/search?q=%s" },
    ecosia:     { name: "Ecosia",     url: "https://www.ecosia.org/search?q=%s" },
    qwant:      { name: "Qwant",      url: "https://www.qwant.com/?q=%s" },
    startpage:  { name: "Startpage",  url: "https://www.startpage.com/do/search?q=%s" },
    yahoo:      { name: "Yahoo",      url: "https://search.yahoo.com/search?p=%s" },
  };
  const THEMES = ["noir", "blanc"];

  const ACCENTS = [
    { id: "auto", name: "Neutre", color: "" },
    { id: "bleu", name: "Bleu", color: "#4c8dff" },
    { id: "violet", name: "Violet", color: "#9b7bff" },
    { id: "vert", name: "Vert", color: "#2fbf71" },
    { id: "orange", name: "Orange", color: "#ff9f0a" },
    { id: "rose", name: "Rose", color: "#ff5c8a" },
  ];
  const TINTS = [
    { id: "none", name: "Neutre", color: "" },
    { id: "nuit", name: "Bleu nuit", color: "#2a4bd7" },
    { id: "ocean", name: "Océan", color: "#0aa5c2" },
    { id: "foret", name: "Forêt", color: "#1f9d55" },
    { id: "violet", name: "Violet", color: "#7c4dff" },
    { id: "bordeaux", name: "Bordeaux", color: "#b3213a" },
    { id: "rose", name: "Rose", color: "#e0457b" },
    { id: "ambre", name: "Ambre", color: "#e08a00" },
  ];
  const hexOk = (h) => typeof h === "string" && /^#[0-9a-f]{6}$/i.test(h);
  const getTint = () => { const t = store.get("tint", "none"); return t === "custom" || TINTS.some((x) => x.id === t) ? t : "none"; };
  const getCustomTint = () => { const c = store.get("tintCustom", "#4c8dff"); return hexOk(c) ? c : "#4c8dff"; };
  const tintHex = () => { const t = getTint(); if (t === "custom") return getCustomTint(); return (TINTS.find((x) => x.id === t) || TINTS[0]).color; };
  const getGlass = () => { const g = store.get("glass", 65); return typeof g === "number" && g >= 0 && g <= 100 ? g : 65; };

  const toRgb = (hex) => { const n = parseInt(hex.slice(1), 16); return [(n >> 16) & 255, (n >> 8) & 255, n & 255]; };
  const mixRgb = (c, t, k) => c.map((v, i) => Math.round(v + (t[i] - v) * k));
  const rgba = (c, a) => "rgba(" + c[0] + "," + c[1] + "," + c[2] + "," + a + ")";
  const TINT_PROPS = ["--glass", "--glass2", "--hover", "--line", "--card", "--modal", "--sheet", "--tint-glow"];

  // Applique la couleur du navigateur et la transparence. Renvoie [r, g, b, a] pour l'effet acrylique de la fenêtre.
  const applyTint = (mode) => {
    const s = document.documentElement.style;
    const dark = mode !== "blanc";
    const g = getGlass() / 100;
    const hex = tintHex();
    const BLACK = [0, 0, 0], WHITE = [255, 255, 255];
    const c = hex ? toRgb(hex) : null;
    const base = c ? (dark ? mixRgb(c, BLACK, 0.6) : mixRgb(c, WHITE, 0.72)) : (dark ? BLACK : WHITE);
    const overlay = dark ? 0.5 - 0.48 * g : 0.7 - 0.62 * g;
    s.setProperty("--bg-tl", rgba(base, overlay.toFixed(3)));
    if (!c) {
      TINT_PROPS.forEach((k) => s.removeProperty(k));
    } else if (dark) {
      s.setProperty("--glass", rgba(c, 0.12));
      s.setProperty("--glass2", rgba(c, 0.26));
      s.setProperty("--hover", rgba(c, 0.22));
      s.setProperty("--line", "rgba(255,255,255,.16)");
      s.setProperty("--card", rgba(mixRgb(c, BLACK, 0.6), 0.58));
      s.setProperty("--modal", rgba(mixRgb(c, BLACK, 0.6), 0.46));
      s.setProperty("--sheet", rgba(mixRgb(c, BLACK, 0.84), 0.9));
      s.setProperty("--tint-glow", rgba(c, 0.20));
    } else {
      s.setProperty("--glass", rgba(mixRgb(c, WHITE, 0.75), 0.30));
      s.setProperty("--glass2", rgba(mixRgb(c, WHITE, 0.6), 0.5));
      s.setProperty("--hover", rgba(mixRgb(c, WHITE, 0.5), 0.3));
      s.setProperty("--line", "rgba(255,255,255,.55)");
      s.setProperty("--card", rgba(mixRgb(c, WHITE, 0.75), 0.5));
      s.setProperty("--modal", rgba(mixRgb(c, WHITE, 0.75), 0.55));
      s.setProperty("--sheet", rgba(mixRgb(c, WHITE, 0.9), 0.92));
      s.setProperty("--tint-glow", rgba(c, 0.22));
    }
    const win = c ? (dark ? mixRgb(c, BLACK, 0.45) : mixRgb(c, WHITE, 0.55)) : (dark ? BLACK : WHITE);
    const alpha = dark ? Math.round(170 - 150 * g) : Math.round(215 - 150 * g);
    return [win[0], win[1], win[2], alpha];
  };

  const getAccent = () => { const a = store.get("accent", "auto"); return ACCENTS.some((x) => x.id === a) ? a : "auto"; };
  const onColor = (hex) => {
    const n = parseInt(hex.slice(1), 16);
    const l = (0.2126 * ((n >> 16) & 255) + 0.7152 * ((n >> 8) & 255) + 0.0722 * (n & 255)) / 255;
    return l > 0.6 ? "#000000" : "#ffffff";
  };
  const applyAccent = () => {
    const s = document.documentElement.style;
    const c = (ACCENTS.find((x) => x.id === getAccent()) || ACCENTS[0]).color || tintHex();
    if (!c) { s.removeProperty("--accent"); s.removeProperty("--onaccent"); return; }
    s.setProperty("--accent", c);
    s.setProperty("--onaccent", onColor(c));
  };

  const getTheme = () => { const t = store.get("theme", "noir"); return THEMES.includes(t) ? t : "noir"; };
  const getEngineId = () => { const id = store.get("engine", "google"); return id === "custom" || ENGINES[id] ? id : "google"; };
  const customUrl = () => { const c = store.get("customEngine", ""); return typeof c === "string" && c.includes("%s") ? c : ""; };

  const KEYWORDS_DEFAULT = [
    { k: "w", url: "https://fr.wikipedia.org/w/index.php?search=%s" },
    { k: "yt", url: "https://www.youtube.com/results?search_query=%s" },
    { k: "gh", url: "https://github.com/search?q=%s" },
    { k: "maps", url: "https://www.google.com/maps/search/%s" },
    { k: "a", url: "https://www.amazon.fr/s?k=%s" },
    { k: "img", url: "https://www.google.com/search?tbm=isch&q=%s" },
  ];
  const validKeyword = (e) => e && typeof e.k === "string" && /^[a-z0-9]{1,12}$/.test(e.k) && typeof e.url === "string" && /^https?:\/\/.+%s/.test(e.url);
  const keywords = () => { const l = store.get("keywords", null); return Array.isArray(l) ? l.filter(validKeyword) : KEYWORDS_DEFAULT.slice(); };
  const saveKeywords = (l) => store.set("keywords", l.filter(validKeyword));

  function engineTemplate() {
    const id = getEngineId();
    if (id === "custom" && customUrl()) return customUrl();
    return (ENGINES[id] || ENGINES.google).url;
  }
  function engineName() {
    const id = getEngineId();
    if (id === "custom" && customUrl()) { try { return new URL(customUrl().replace("%s", "x")).hostname; } catch {} }
    return (ENGINES[id] || ENGINES.google).name;
  }

  function resolve(input) {
    const s = (input || "").trim();
    if (!s) return "";
    if (/^(https?|file):\/\//i.test(s)) return s;
    const m = /^(\S+)\s+(.+)$/.exec(s);
    if (m) {
      const kw = keywords().find((e) => e.k === m[1].toLowerCase());
      if (kw) return kw.url.replace("%s", encodeURIComponent(m[2].trim()));
    }
    if (!/\s/.test(s) && (s.includes(".") || /^localhost/i.test(s))) return "https://" + s;
    return engineTemplate().replace("%s", encodeURIComponent(s));
  }

  const HOME_DEFAULTS = { clock: true, date: true, tiles: true, source: "bookmarks", custom: [], weather: "" };
  const homeConfig = () => Object.assign({}, HOME_DEFAULTS, store.get("home", {}));
  const saveHome = (patch) => store.set("home", Object.assign(homeConfig(), patch));

  const isArr = Array.isArray;
  const isObj = (v) => v && typeof v === "object" && !isArr(v);
  const isBool = (v) => typeof v === "boolean";
  const isStr = (v) => typeof v === "string" && v.length < 2000;
  const validApp = (a) => isObj(a) && isStr(a.key) && isStr(a.name) && isStr(a.url);
  const SCHEMA = {
    theme: (v) => THEMES.includes(v),
    accent: (v) => ACCENTS.some((a) => a.id === v),
    engine: (v) => v === "custom" || !!ENGINES[v],
    customEngine: isStr,
    keywords: (v) => isArr(v) && v.length <= 40 && v.every(validKeyword),
    keys: isObj,
    restoreSession: isBool,
    adblock: isBool,
    adblockAllow: (v) => isArr(v) && v.length <= 200 && v.every((h) => typeof h === "string" && /^[a-z0-9.-]{1,120}$/.test(h)),
    autoCheckUpdates: isBool,
    zenStart: isBool,
    newTab: (v) => ["home", "blank", "url"].includes(v),
    newTabUrl: isStr,
    clearOnExit: isBool,
    tint: (v) => v === "custom" || TINTS.some((t) => t.id === v),
    tintCustom: hexOk,
    glass: (v) => typeof v === "number" && v >= 0 && v <= 100,
    hwAccel: isBool,
    dlAsk: isBool,
    bookmarks: (v) => isArr(v) && v.length <= 5000 && v.every((b) => isObj(b) && isStr(b.url)),
    history: (v) => isArr(v) && v.length <= 20000 && v.every((b) => isObj(b) && isStr(b.url)),
    sidebarApps: (v) => isArr(v) && v.length <= 60 && v.every((a) => validApp(a) || (isObj(a) && a.folder === true && isStr(a.key) && isStr(a.name) && isArr(a.items) && a.items.length <= 40 && a.items.every(validApp))),
    home: isObj,
  };

  const exportAll = () => {
    const data = {};
    for (const k of Object.keys(SCHEMA)) {
      const v = store.get(k, undefined);
      if (v !== undefined) data[k] = v;
    }
    return { app: "tweax", version: 1, date: new Date().toISOString(), data };
  };
  const importAll = (obj) => {
    if (!isObj(obj) || obj.app !== "tweax" || !isObj(obj.data)) return -1;
    let n = 0;
    for (const [k, v] of Object.entries(obj.data)) {
      if (SCHEMA[k] && SCHEMA[k](v)) { store.set(k, v); n++; }
    }
    return n;
  };

  window.TX = {
    homeConfig, saveHome, store, ENGINES, THEMES, getTheme, getEngineId, customUrl, engineName, resolve,
    TINTS, getTint, getCustomTint, getGlass, applyTint,
    ACCENTS, getAccent, applyAccent, keywords, saveKeywords, KEYWORDS_DEFAULT,
    profiles, saveProfiles, activeProfile, profileName, setProfile, newProfileId, dropProfileData, validId,
    exportAll, importAll,
  };
})();
