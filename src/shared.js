/* Tweax — réglages partagés entre l'interface (index.html) et la page d'accueil (home.html) */
(() => {
  const store = {
    get(k, d) { try { const v = localStorage.getItem(k); return v ? JSON.parse(v) : d; } catch { return d; } },
    set(k, v) { try { localStorage.setItem(k, JSON.stringify(v)); } catch {} },
  };

  // %s est remplacé par la recherche de l'utilisateur
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

  const getTheme = () => { const t = store.get("theme", "noir"); return THEMES.includes(t) ? t : "noir"; };
  const getEngineId = () => { const id = store.get("engine", "google"); return id === "custom" || ENGINES[id] ? id : "google"; };
  const customUrl = () => { const c = store.get("customEngine", ""); return typeof c === "string" && c.includes("%s") ? c : ""; };

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

  // Transforme ce que tape l'utilisateur en adresse (ou en recherche avec SON moteur)
  function resolve(input) {
    const s = (input || "").trim();
    if (!s) return "";
    if (/^(https?|file):\/\//i.test(s)) return s;
    if (!/\s/.test(s) && (s.includes(".") || /^localhost/i.test(s))) return "https://" + s;
    return engineTemplate().replace("%s", encodeURIComponent(s));
  }

  window.TX = { store, ENGINES, THEMES, getTheme, getEngineId, customUrl, engineName, resolve };
})();
