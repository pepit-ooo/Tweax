mod vault;

use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::time::{Duration, Instant};
use std::sync::Mutex;

use tauri::{State,
    webview::{PageLoadEvent, WebviewBuilder},
    window::WindowBuilder,
    AppHandle, Emitter, LogicalPosition, LogicalSize, Manager, Url, Webview, WebviewUrl,
    WindowEvent,
};

// ---------- Géométrie de l'interface (pixels logiques) ----------
/// Barre latérale d'icônes (style Opera), à gauche sur toute la hauteur
const RAIL_W: f64 = 58.0;
/// Hauteur de la zone du haut : rangée d'onglets + barre d'outils
const TOP_H: f64 = 94.0;
/// Largeur du panneau latéral quand il est ouvert
const PANEL_W: f64 = 380.0;
/// Durée de l'animation de rétractation (identique à la transition CSS de l'interface)
const ZEN_MS: f64 = 240.0;

const HOME: &str = "https://www.google.com/";

static NEXT_ID: AtomicU32 = AtomicU32::new(1);
static PANEL_OPEN: AtomicBool = AtomicBool::new(false);

// Mode "page en grand" : la barre latérale et la zone du haut se rétractent.
// RAIL_CUR / TOP_CUR = tailles actuelles (animées) en pixels logiques, stockées en bits de f64.
static ZEN: AtomicBool = AtomicBool::new(false);
// Le masquage automatique peut être suspendu lorsque l’utilisateur quitte manuellement le mode agrandi.
static AUTO_HIDE_ENABLED: AtomicBool = AtomicBool::new(false);
static ZEN_GEN: AtomicU32 = AtomicU32::new(0); // numéro de l'animation en cours (annule les anciennes)
static RAIL_CUR: AtomicU64 = AtomicU64::new(RAIL_W.to_bits());
static TOP_CUR: AtomicU64 = AtomicU64::new(TOP_H.to_bits());

fn rail_cur() -> f64 {
    f64::from_bits(RAIL_CUR.load(Ordering::SeqCst))
}
fn top_cur() -> f64 {
    f64::from_bits(TOP_CUR.load(Ordering::SeqCst))
}

#[derive(Clone, serde::Serialize)]
struct TabUpdate {
    id: u32,
    url: String,
    loading: bool,
}

/// Transforme ce que tape l'utilisateur en URL valide (ou en recherche)
fn normalize(input: &str) -> Url {
    let s = input.trim();
    if s.is_empty() {
        return Url::parse(HOME).unwrap();
    }
    if s.starts_with("http://") || s.starts_with("https://") || s.starts_with("file://") {
        if let Ok(u) = Url::parse(s) {
            return u;
        }
    }
    if !s.contains(' ') && (s.contains('.') || s.starts_with("localhost")) {
        if let Ok(u) = Url::parse(&format!("https://{s}")) {
            return u;
        }
    }
    // Repli : normalement l'interface a déjà transformé la saisie avec le moteur choisi
    Url::parse_with_params("https://www.google.com/search", &[("q", s)]).unwrap()
}

/// Vrai si l'adresse est la page d'accueil intégrée de Tweax (et pas un site qui aurait le même chemin)
#[allow(dead_code)]
fn is_home_url(u: &Url) -> bool {
    let local = u.scheme() == "tauri" || matches!(u.host_str(), Some("tauri.localhost") | Some("localhost"));
    local && u.path() == "/home.html"
}

fn tab(app: &AppHandle, id: u32) -> Result<Webview, String> {
    app.get_webview(&format!("tab-{id}"))
        .ok_or_else(|| format!("onglet {id} introuvable"))
}

/// (x, y, largeur, hauteur) de la page web de l'onglet actif : collée à l'interface, sans marge ni cadre
fn content_rect(win_w: f64, win_h: f64) -> (f64, f64, f64, f64) {
    let left = rail_cur() + if PANEL_OPEN.load(Ordering::SeqCst) { PANEL_W } else { 0.0 };
    let top = top_cur();
    (left, top, (win_w - left).max(1.0), (win_h - top).max(1.0))
}

/// (x, y, largeur, hauteur) du panneau latéral
fn panel_rect(win_h: f64) -> (f64, f64, f64, f64) {
    let top = top_cur();
    (rail_cur(), top, PANEL_W, (win_h - top).max(1.0))
}

fn place(wv: &Webview, r: (f64, f64, f64, f64)) {
    let _ = wv.set_position(LogicalPosition::new(r.0, r.1));
    let _ = wv.set_size(LogicalSize::new(r.2, r.3));
}

fn window_size(app: &AppHandle) -> Option<(f64, f64)> {
    let window = app.get_window("main")?;
    let scale = window.scale_factor().ok()?;
    let s = window.inner_size().ok()?.to_logical::<f64>(scale);
    Some((s.width, s.height))
}

/// Replace l'interface, les panneaux et les onglets selon la taille de la fenêtre
fn layout(app: &AppHandle) {
    let Some((w, h)) = window_size(app) else { return };
    for (label, wv) in app.webviews() {
        if label == "ui" {
            place(&wv, (0.0, 0.0, w, h));
        } else if label.starts_with("panel-") {
            place(&wv, panel_rect(h));
        } else {
            place(&wv, content_rect(w, h));
        }
    }
}

fn hide_all_tabs(app: &AppHandle) {
    for (label, wv) in app.webviews() {
        if label.starts_with("tab-") {
            let _ = wv.hide();
        }
    }
}

fn hide_all_panels(app: &AppHandle) {
    for (label, wv) in app.webviews() {
        if label.starts_with("panel-") {
            let _ = wv.hide();
        }
    }
}

/// Affiche une petite bulle directement dans chaque page (au-dessus du site),
/// pour indiquer les bords à survoler lorsque les barres sont repliées.
fn set_edge_hint(wv: &Webview, show: bool) {
    let js = if show {
        r#"(() => {
          let h = document.getElementById('tweax-edge-hint');
          if (!h) {
            h = document.createElement('div');
            h.id = 'tweax-edge-hint';
            h.textContent = '↑ Survolez le bord supérieur ou gauche pour réafficher les barres';
            Object.assign(h.style, {
              position:'fixed', top:'12px', left:'50%', transform:'translateX(-50%)',
              zIndex:'2147483647', padding:'9px 14px',
              border:'1px solid rgba(255,255,255,.22)', borderRadius:'999px',
              background:'rgba(18,18,18,.48)', color:'#fff',
              font:'12px Segoe UI, sans-serif', boxShadow:'0 5px 22px rgba(0,0,0,.22)',
              pointerEvents:'none', backdropFilter:'blur(22px)',
              opacity:'0', transition:'opacity .22s ease'
            });
            (document.documentElement || document.body).appendChild(h);
            requestAnimationFrame(() => { h.style.opacity = '1'; });
          }
          clearTimeout(window.__tweaxHintTimer);
          window.__tweaxHintTimer = setTimeout(() => {
            const hint = document.getElementById('tweax-edge-hint');
            if (hint) {
              hint.style.opacity = '0';
              setTimeout(() => hint.remove(), 240);
            }
          }, 5000);
        })();"#
    } else {
        r#"clearTimeout(window.__tweaxHintTimer); document.getElementById('tweax-edge-hint')?.remove();"#
    };
    let _ = wv.eval(js);
}

/// Surveille les bords de la fenêtre : quand le mode automatique est actif,
/// le survol du haut/gauche affiche temporairement les barres, puis elles se replient
/// quand le curseur quitte les zones de chrome. Cela ne désactive jamais le mode.
fn watch_zen_edges(app: AppHandle) {
    std::thread::spawn(move || {
        let mut away_since: Option<Instant> = None;
        let mut sent_enter = false;
        let mut sent_exit = false;
        loop {
            std::thread::sleep(Duration::from_millis(60));
            let Some(win) = app.get_window("main") else { return };
            let (Ok(cur), Ok(pos), Ok(size)) = (win.cursor_position(), win.inner_position(), win.inner_size()) else { continue };
            let x = cur.x - pos.x as f64;
            let y = cur.y - pos.y as f64;
            let inside = x >= 0.0 && y >= 0.0 && x < size.width as f64 && y < size.height as f64;
            if !inside { away_since = None; sent_enter = false; sent_exit = false; continue; }
            if ZEN.load(Ordering::SeqCst) {
                away_since = None; sent_enter = false;
                if (x <= 14.0 || y <= 14.0) && !sent_exit {
                    sent_exit = true; let _ = app.emit("zen-exit", ());
                } else if x > 24.0 && y > 24.0 { sent_exit = false; }
            } else {
                sent_exit = false;
                let in_chrome = x <= RAIL_W + 4.0 || y <= TOP_H + 4.0;
                if in_chrome || !AUTO_HIDE_ENABLED.load(Ordering::SeqCst) {
                    away_since = None;
                    sent_enter = false;
                } else {
                    if away_since.is_none() { away_since = Some(Instant::now()); }
                    if !sent_enter && away_since.map(|t| t.elapsed() >= Duration::from_millis(750)).unwrap_or(false) {
                        sent_enter = true; let _ = app.emit("zen-enter", ());
                    }
                }
            }
        }
    });
}

// ---------- Commandes appelées depuis l'interface ----------

#[tauri::command]
async fn new_tab(app: AppHandle, url: Option<String>) -> Result<u32, String> {
    let window = app.get_window("main").ok_or("fenêtre introuvable")?;
    let id = NEXT_ID.fetch_add(1, Ordering::SeqCst);
    let url = url.unwrap_or_default();
    // Pas d'adresse -> page d'accueil intégrée de Tweax (src/home.html)
    let is_home = url.trim().is_empty();
    let target = if is_home {
        WebviewUrl::App("home.html".into())
    } else {
        WebviewUrl::External(normalize(&url))
    };

    hide_all_tabs(&app);

    let (w, h) = window_size(&app).ok_or("taille introuvable")?;
    let (x, y, cw, ch) = content_rect(w, h);

    let handle = app.clone();
    let popup_handle = app.clone();
    let builder = WebviewBuilder::new(format!("tab-{id}"), target)
        // target="_blank" / window.open : on refuse la fenêtre native et l'interface ouvre un onglet
        .on_new_window(move |url, _features| {
            let _ = popup_handle.emit("new-window", url.to_string());
            tauri::webview::NewWindowResponse::Deny
        })
        // Les pages sont transparentes sous Windows pour que l'accueil laisse voir l'acrylique ;
        // les vrais sites, eux, reprennent un fond blanc opaque comme dans n'importe quel navigateur.
        .transparent(cfg!(target_os = "windows"))
        // Fond correct dès la création (évite un flash blanc avant le premier chargement)
        .background_color(if is_home && cfg!(target_os = "windows") {
            tauri::webview::Color(0, 0, 0, 0)
        } else {
            tauri::webview::Color(255, 255, 255, 255)
        })
        .on_page_load(move |wv, payload| {
            #[cfg(target_os = "windows")]
            {
                use tauri::webview::Color;
                // L'accueil doit laisser passer l'Acrylic de la fenêtre native.
                // Un fond opaque ici recouvre le verre et produit le grand rectangle blanc/gris.
                if is_home_url(payload.url()) {
                    let _ = wv.set_background_color(Some(Color(0, 0, 0, 0)));
                } else {
                    // Les sites web gardent leur rendu normal et leur fond propre.
                    let _ = wv.set_background_color(Some(Color(255, 255, 255, 255)));
                }
            }
            if ZEN.load(Ordering::SeqCst) { set_edge_hint(&wv, true); }
            #[cfg(not(target_os = "windows"))]
            let _ = &wv;
            let _ = handle.emit(
                "tab-updated",
                TabUpdate {
                    id,
                    url: payload.url().to_string(),
                    loading: matches!(payload.event(), PageLoadEvent::Started),
                },
            );
        });

    let wv = window
        .add_child(builder, LogicalPosition::new(x, y), LogicalSize::new(cw, ch))
        .map_err(|e| e.to_string())?;
    // L'accueil est dessiné par l'interface (iframe transparente) : la webview native reste cachée
    if is_home {
        let _ = wv.hide();
    }

    Ok(id)
}

#[tauri::command]
async fn switch_tab(app: AppHandle, id: u32) -> Result<(), String> {
    let wv = tab(&app, id)?;
    hide_all_tabs(&app);
    wv.show().map_err(|e| e.to_string())?;
    let _ = wv.set_focus();
    Ok(())
}

/// Cache toutes les pages natives sans toucher aux panneaux : l'interface affiche alors l'accueil elle-même
#[tauri::command]
async fn hide_all_tabs_cmd(app: AppHandle) -> Result<(), String> {
    hide_all_tabs(&app);
    if let Some(ui) = app.get_webview("ui") {
        let _ = ui.set_focus();
    }
    Ok(())
}

#[tauri::command]
async fn close_tab(app: AppHandle, id: u32) -> Result<(), String> {
    tab(&app, id)?.close().map_err(|e| e.to_string())
}

#[tauri::command]
async fn navigate(app: AppHandle, id: u32, url: String) -> Result<(), String> {
    tab(&app, id)?
        .navigate(normalize(&url))
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn go_back(app: AppHandle, id: u32) -> Result<(), String> {
    tab(&app, id)?.eval("history.back()").map_err(|e| e.to_string())
}

#[tauri::command]
async fn go_forward(app: AppHandle, id: u32) -> Result<(), String> {
    tab(&app, id)?.eval("history.forward()").map_err(|e| e.to_string())
}

#[tauri::command]
async fn reload(app: AppHandle, id: u32) -> Result<(), String> {
    tab(&app, id)?.eval("location.reload()").map_err(|e| e.to_string())
}

/// Ramène l'onglet sur la page d'accueil Tweax
#[tauri::command]
async fn go_home(app: AppHandle, id: u32) -> Result<(), String> {
    let base = app
        .get_webview("ui")
        .ok_or("interface introuvable")?
        .url()
        .map_err(|e| e.to_string())?;
    let home = base.join("home.html").map_err(|e| e.to_string())?;
    tab(&app, id)?.navigate(home).map_err(|e| e.to_string())
}

// ---------- Contrôles de fenêtre (la barre de titre native est retirée) ----------

fn main_window(app: &AppHandle) -> Result<tauri::Window, String> {
    app.get_window("main").ok_or_else(|| "fenêtre introuvable".to_string())
}

#[tauri::command]
async fn win_minimize(app: AppHandle) -> Result<(), String> {
    main_window(&app)?.minimize().map_err(|e| e.to_string())
}

#[tauri::command]
async fn win_toggle_max(app: AppHandle) -> Result<(), String> {
    let w = main_window(&app)?;
    if w.is_maximized().map_err(|e| e.to_string())? {
        w.unmaximize().map_err(|e| e.to_string())
    } else {
        w.maximize().map_err(|e| e.to_string())
    }
}

#[tauri::command]
async fn win_close(app: AppHandle) -> Result<(), String> {
    app.exit(0);
    Ok(())
}

#[tauri::command]
async fn win_drag(app: AppHandle) -> Result<(), String> {
    main_window(&app)?.start_dragging().map_err(|e| e.to_string())
}

/// Translucidité de la fenêtre (acrylique, Windows) : teinte sombre ou claire selon le thème
#[tauri::command]
async fn set_tint(app: AppHandle, dark: bool) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        use tauri::window::{Color, Effect, EffectsBuilder};
        let color = if dark { Color(0, 0, 0, 78) } else { Color(255, 255, 255, 225) };
        main_window(&app)?
            .set_effects(EffectsBuilder::new().effect(Effect::Acrylic).color(color).build())
            .map_err(|e| e.to_string())?;
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = (app, dark);
    }
    Ok(())
}

/// Masque toutes les pages natives (elles passent au-dessus de l'interface) :
/// nécessaire pour afficher la fenêtre de réglages par-dessus.
#[tauri::command]
async fn hide_tabs(app: AppHandle) -> Result<(), String> {
    hide_all_tabs(&app);
    hide_all_panels(&app);
    if let Some(ui) = app.get_webview("ui") {
        let _ = ui.set_focus();
    }
    Ok(())
}

/// Passe la page en grand (true) ou ramène les barres (false), avec une animation :
/// les webviews natives ne peuvent pas être animées en CSS, on interpole donc leur position ici.
#[tauri::command]
async fn set_zen(app: AppHandle, on: bool, auto_hide: Option<bool>) -> Result<(), String> {
    ZEN.store(on, Ordering::SeqCst);
    if let Some(enabled) = auto_hide {
        AUTO_HIDE_ENABLED.store(enabled, Ordering::SeqCst);
    }
    let gen = ZEN_GEN.fetch_add(1, Ordering::SeqCst) + 1;
    let (from_r, from_t) = (rail_cur(), top_cur());
    let (to_r, to_t) = if on { (0.0, 0.0) } else { (RAIL_W, TOP_H) };
    for (label, wv) in app.webviews() {
        if label.starts_with("tab-") { set_edge_hint(&wv, on); }
    }
    std::thread::spawn(move || {
        let start = Instant::now();
        loop {
            if ZEN_GEN.load(Ordering::SeqCst) != gen {
                return; // une animation plus récente a pris le relais
            }
            let t = (start.elapsed().as_secs_f64() * 1000.0 / ZEN_MS).min(1.0);
            let e = 1.0 - (1.0 - t).powi(3); // ease-out cubique (≈ la courbe CSS)
            RAIL_CUR.store((from_r + (to_r - from_r) * e).to_bits(), Ordering::SeqCst);
            TOP_CUR.store((from_t + (to_t - from_t) * e).to_bits(), Ordering::SeqCst);
            layout(&app);
            if t >= 1.0 {
                break;
            }
            std::thread::sleep(Duration::from_millis(8));
        }
    });
    Ok(())
}

/// Ouvre / ferme la zone du panneau latéral (la page se décale en conséquence)
#[tauri::command]
async fn set_panel(app: AppHandle, open: bool) -> Result<(), String> {
    PANEL_OPEN.store(open, Ordering::SeqCst);
    if !open {
        hide_all_panels(&app);
    }
    layout(&app);
    Ok(())
}

/// Affiche un site web dans le panneau latéral (WhatsApp, Discord…).
/// Chaque site garde sa session : on le crée une fois, puis on le montre/cache.
#[tauri::command]
async fn show_web_panel(app: AppHandle, key: String, url: String) -> Result<(), String> {
    let window = app.get_window("main").ok_or("fenêtre introuvable")?;
    let label = format!("panel-{key}");
    let (_, h) = window_size(&app).ok_or("taille introuvable")?;

    hide_all_panels(&app);
    PANEL_OPEN.store(true, Ordering::SeqCst);

    if let Some(wv) = app.get_webview(&label) {
        wv.show().map_err(|e| e.to_string())?;
    } else {
        let r = panel_rect(h);
        window
            .add_child(
                WebviewBuilder::new(label, WebviewUrl::External(normalize(&url))),
                LogicalPosition::new(r.0, r.1),
                LogicalSize::new(r.2, r.3),
            )
            .map_err(|e| e.to_string())?;
    }
    layout(&app);
    Ok(())
}

/// Cache les sites du panneau (pour afficher un panneau interne : favoris, historique…)
#[tauri::command]
async fn hide_web_panels(app: AppHandle) -> Result<(), String> {
    hide_all_panels(&app);
    Ok(())
}

/// Supprime la session d'un site du panneau (quand on retire l'icône)
#[tauri::command]
async fn close_web_panel(app: AppHandle, key: String) -> Result<(), String> {
    if let Some(wv) = app.get_webview(&format!("panel-{key}")) {
        wv.close().map_err(|e| e.to_string())?;
    }
    Ok(())
}

// ---------- Coffre de mots de passe + import depuis un autre navigateur ----------
// Les commandes sont `async` : Argon2 prend ~0,3 s et ne doit pas bloquer le fil principal.

struct VaultState(Mutex<vault::Vault>);

fn with_vault<R>(
    state: &State<'_, VaultState>,
    f: impl FnOnce(&mut vault::Vault) -> Result<R, String>,
) -> Result<R, String> {
    let mut v = state.0.lock().map_err(|_| "coffre indisponible".to_string())?;
    f(&mut v)
}

#[tauri::command]
async fn vault_status(state: State<'_, VaultState>) -> Result<vault::Status, String> {
    with_vault(&state, |v| Ok(v.status()))
}

#[tauri::command]
async fn vault_create(state: State<'_, VaultState>, master: String) -> Result<(), String> {
    with_vault(&state, |v| v.create(&master))
}

#[tauri::command]
async fn vault_unlock(state: State<'_, VaultState>, master: String) -> Result<(), String> {
    with_vault(&state, |v| v.unlock(&master))
}

#[tauri::command]
async fn vault_lock(state: State<'_, VaultState>) -> Result<(), String> {
    with_vault(&state, |v| {
        v.lock();
        Ok(())
    })
}

#[tauri::command]
async fn vault_list(state: State<'_, VaultState>) -> Result<Vec<vault::PublicEntry>, String> {
    with_vault(&state, |v| v.list())
}

#[tauri::command]
async fn vault_reveal(state: State<'_, VaultState>, id: String) -> Result<String, String> {
    with_vault(&state, |v| v.reveal(&id))
}

#[tauri::command]
async fn vault_delete(state: State<'_, VaultState>, id: String) -> Result<(), String> {
    with_vault(&state, |v| v.delete(&id))
}

/// Étape 1 de l'import : analyse l'export CSV d'un autre navigateur (rien n'est encore enregistré)
#[tauri::command]
async fn vault_import_preview(state: State<'_, VaultState>, csv: String) -> Result<vault::Preview, String> {
    with_vault(&state, |v| v.import_preview(&csv))
}

/// Étape 2 : enregistre l'import analysé
#[tauri::command]
async fn vault_import_commit(state: State<'_, VaultState>, replace_conflicts: bool) -> Result<vault::ImportResult, String> {
    with_vault(&state, |v| v.import_commit(replace_conflicts))
}

#[tauri::command]
async fn vault_save(
    state: State<'_, VaultState>,
    id: Option<String>,
    name: String,
    url: String,
    username: String,
    password: String,
    note: Option<String>,
) -> Result<String, String> {
    with_vault(&state, |v| v.save_entry(id, &name, &url, &username, &password, note))
}

#[tauri::command]
async fn vault_change_master(state: State<'_, VaultState>, old: String, new: String) -> Result<(), String> {
    with_vault(&state, |v| v.change_master(&old, &new))
}

#[tauri::command]
async fn vault_audit(state: State<'_, VaultState>) -> Result<Vec<vault::AuditItem>, String> {
    with_vault(&state, |v| v.audit())
}

/// Identifiants du coffre qui correspondent (exactement) au site affiché
#[tauri::command]
async fn vault_matches(state: State<'_, VaultState>, host: String) -> Result<Vec<vault::PublicEntry>, String> {
    with_vault(&state, |v| v.matches_host(&host))
}

/// Script exécuté dans la page : trouve le champ mot de passe visible + le champ identifiant qui le précède
const AUTOFILL_JS: &str = r#"(function (u, p) {
  var vis = function (e) { var r = e.getBoundingClientRect(); return r.width > 0 && r.height > 0 && getComputedStyle(e).visibility !== 'hidden'; };
  var set = function (el, v) {
    Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value').set.call(el, v);
    el.dispatchEvent(new Event('input', { bubbles: true }));
    el.dispatchEvent(new Event('change', { bubbles: true }));
  };
  var all = Array.prototype.slice.call(document.querySelectorAll('input')).filter(function (e) { return vis(e) && !e.disabled && !e.readOnly; });
  var pw = all.filter(function (e) { return e.type === 'password'; })[0];
  var texts = all.filter(function (e) { return ['text', 'email', 'tel', ''].indexOf((e.getAttribute('type') || '').toLowerCase()) >= 0; });
  var user = null;
  if (pw) {
    var before = texts.filter(function (e) { return pw.compareDocumentPosition(e) & Node.DOCUMENT_POSITION_PRECEDING; });
    user = before[before.length - 1] || null;
  } else { user = texts[0] || null; }
  if (user && u) set(user, u);
  if (pw) set(pw, p);
})"#;

/// Remplit la page de l'onglet avec un identifiant du coffre. Déclenché par l'utilisateur (clic sur la clé).
/// Garde-fous : la page doit être en HTTPS (ou localhost) ET son hôte doit être exactement celui de l'entrée.
#[tauri::command]
async fn vault_autofill(app: AppHandle, state: State<'_, VaultState>, tab_id: u32, entry_id: String) -> Result<(), String> {
    let wv = tab(&app, tab_id)?;
    let page = wv.url().map_err(|e| e.to_string())?;
    let secure = page.scheme() == "https" || page.host_str() == Some("localhost");
    if !secure {
        return Err("page non sécurisée (HTTP) : remplissage refusé".into());
    }
    let (url, user, pass) = with_vault(&state, |v| v.credentials(&entry_id))?;
    if vault::site_key(&url) != vault::site_key(page.as_str()) {
        return Err("cet identifiant n'appartient pas à ce site".into());
    }
    let script = format!(
        "{}({},{});",
        AUTOFILL_JS,
        serde_json::to_string(&user).map_err(|e| e.to_string())?,
        serde_json::to_string(&pass).map_err(|e| e.to_string())?
    );
    wv.eval(&script).map_err(|e| e.to_string())
}

#[tauri::command]
async fn vault_import_cancel(state: State<'_, VaultState>) -> Result<(), String> {
    with_vault(&state, |v| {
        v.import_cancel();
        Ok(())
    })
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_process::init())
        .setup(|app| {
            // Mises à jour : le plugin n'est activé que si "plugins.updater" est présent dans la config
            // (fusionnée au build de release via tauri.release.conf.json). En dev, il est ignoré.
            #[cfg(desktop)]
            {
                if app.config().plugins.0.contains_key("updater") {
                    app.handle().plugin(tauri_plugin_updater::Builder::new().build())?;
                }
            }

            // Coffre : fichier chiffré dans le dossier de données de l'application
            let dir = app.path().app_data_dir()?;
            std::fs::create_dir_all(&dir)?;
            app.manage(VaultState(Mutex::new(vault::Vault::new(dir.join("vault.bin")))));

            let mut builder = WindowBuilder::new(app, "main")
                .title("Tweax")
                .inner_size(1280.0, 820.0)
                .min_inner_size(760.0, 420.0)
                .decorations(false) // plus de barre de titre native
                .shadow(true)
                .transparent(cfg!(target_os = "windows"));
            if let Some(icon) = app.default_window_icon() {
                builder = builder.icon(icon.clone())?;
            }
            let window = builder.build()?;

            let size = window
                .inner_size()?
                .to_logical::<f64>(window.scale_factor()?);

            // La webview "ui" couvre toute la fenêtre (fond, barre latérale, onglets,
            // barre d'adresse). Les pages web sont posées PAR-DESSUS, dans la "carte".
            window.add_child(
                WebviewBuilder::new("ui", WebviewUrl::App("index.html".into()))
                    .transparent(cfg!(target_os = "windows")),
                LogicalPosition::new(0.0, 0.0),
                LogicalSize::new(size.width, size.height),
            )?;

            watch_zen_edges(app.handle().clone());
            Ok(())
        })
        .on_window_event(|window, event| {
            if let WindowEvent::Resized(_) = event {
                layout(window.app_handle());
            }
        })
        .invoke_handler(tauri::generate_handler![
            new_tab,
            switch_tab,
            hide_all_tabs_cmd,
            close_tab,
            navigate,
            go_back,
            go_forward,
            reload,
            go_home,
            win_minimize,
            win_toggle_max,
            win_close,
            win_drag,
            set_tint,
            set_panel,
            set_zen,
            hide_tabs,
            show_web_panel,
            hide_web_panels,
            close_web_panel,
            vault_status,
            vault_create,
            vault_unlock,
            vault_lock,
            vault_list,
            vault_reveal,
            vault_delete,
            vault_import_preview,
            vault_import_commit,
            vault_import_cancel,
            vault_save,
            vault_change_master,
            vault_audit,
            vault_matches,
            vault_autofill
        ])
        .run(tauri::generate_context!())
        .expect("erreur au lancement de l'application");
}
