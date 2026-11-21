mod vault;
mod adblock_native;

use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::time::{Duration, Instant};
use std::sync::Mutex;
use std::path::{Path, PathBuf};

use tauri::{State,
    webview::{DownloadEvent, PageLoadEvent, WebviewBuilder},
    window::WindowBuilder,
    AppHandle, Emitter, LogicalPosition, LogicalSize, Manager, Rect, Url, Webview, WebviewUrl,
    WindowEvent,
};

const RAIL_W: f64 = 58.0;
const TOP_H: f64 = 94.0;
const PANEL_W: f64 = 380.0;
const ZEN_MS: f64 = 240.0;

const HOME: &str = "https://www.google.com/";

static NEXT_ID: AtomicU32 = AtomicU32::new(1);
static ADBLOCK: AtomicBool = AtomicBool::new(false);
static KEYS: Mutex<Vec<String>> = Mutex::new(Vec::new());
static ALLOW: Mutex<Vec<String>> = Mutex::new(Vec::new());
static PROFILE: Mutex<String> = Mutex::new(String::new());
static DL_DIR: Mutex<Option<PathBuf>> = Mutex::new(None);
static DL_ASK: AtomicBool = AtomicBool::new(false);
static CLEAR_ON_EXIT: AtomicBool = AtomicBool::new(false);
static ACTIVE_TAB: AtomicU32 = AtomicU32::new(0);
static ACTIVE_PANEL: Mutex<Option<String>> = Mutex::new(None);

const PAGE_JS: &str = include_str!("page.js");
const ADBLOCK_JS: &str = include_str!("adblock.js");

fn keys_snapshot() -> Vec<String> {
    KEYS.lock().map(|k| k.clone()).unwrap_or_default()
}

fn allow_snapshot() -> Vec<String> {
    ALLOW.lock().map(|k| k.clone()).unwrap_or_default()
}

fn valid_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 40 && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

fn profile_id() -> String {
    let p = PROFILE.lock().map(|p| p.clone()).unwrap_or_default();
    if valid_id(&p) { p } else { "default".to_string() }
}

fn browsing_root(app: &AppHandle) -> Option<PathBuf> {
    app.path().app_local_data_dir().ok().map(|d| d.join("browsing"))
}

fn profile_dir(app: &AppHandle) -> Option<PathBuf> {
    let dir = browsing_root(app)?.join(profile_id());
    std::fs::create_dir_all(&dir).ok()?;
    Some(dir)
}

fn clear_data(app: &AppHandle) -> Result<(), String> {
    for (label, wv) in app.webviews() {
        if label.starts_with("tab-") || label.starts_with("panel-") {
            return wv.clear_all_browsing_data().map_err(|e| e.to_string());
        }
    }
    Ok(())
}

fn page_script() -> String {
    let cfg = serde_json::json!({
        "keys": keys_snapshot(),
        "allow": allow_snapshot(),
    });
    let extra = if ADBLOCK.load(Ordering::SeqCst) { ADBLOCK_JS } else { "" };
    format!("window.__TWEAX_CFG = {cfg};\n{PAGE_JS}{extra}")
}

fn unique_path(dir: &Path, name: &std::ffi::OsStr) -> PathBuf {
    let first = dir.join(name);
    if !first.exists() {
        return first;
    }
    let stem = Path::new(name).file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    let ext = Path::new(name).extension().map(|e| format!(".{}", e.to_string_lossy())).unwrap_or_default();
    let mut n = 1;
    loop {
        let candidate = dir.join(format!("{stem} ({n}){ext}"));
        if !candidate.exists() {
            return candidate;
        }
        n += 1;
    }
}

#[derive(Clone, serde::Serialize)]
struct DownloadInfo {
    url: String,
    path: Option<String>,
    success: Option<bool>,
}
static PANEL_OPEN: AtomicBool = AtomicBool::new(false);

static ZEN: AtomicBool = AtomicBool::new(false);
static AUTO_HIDE_ENABLED: AtomicBool = AtomicBool::new(false);
static ZEN_GEN: AtomicU32 = AtomicU32::new(0);
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

fn normalize(input: &str) -> Url {
    let s = input.trim();
    if s.is_empty() {
        return Url::parse(HOME).unwrap();
    }
    if s == "about:blank" {
        return Url::parse("about:blank").unwrap();
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
    Url::parse_with_params("https://www.google.com/search", &[("q", s)]).unwrap()
}

#[allow(dead_code)]
fn is_home_url(u: &Url) -> bool {
    let local = u.scheme() == "tauri" || matches!(u.host_str(), Some("tauri.localhost") | Some("localhost"));
    local && u.path() == "/home.html"
}

fn tab(app: &AppHandle, id: u32) -> Result<Webview, String> {
    app.get_webview(&format!("tab-{id}"))
        .ok_or_else(|| format!("onglet {id} introuvable"))
}

fn content_rect(win_w: f64, win_h: f64) -> (f64, f64, f64, f64) {
    let left = rail_cur() + if PANEL_OPEN.load(Ordering::SeqCst) { PANEL_W } else { 0.0 };
    let top = top_cur();
    (left, top, (win_w - left).max(1.0), (win_h - top).max(1.0))
}

fn panel_rect(win_h: f64) -> (f64, f64, f64, f64) {
    let top = top_cur();
    (rail_cur(), top, PANEL_W, (win_h - top).max(1.0))
}

fn place(wv: &Webview, r: (f64, f64, f64, f64)) {
    let _ = wv.set_bounds(Rect {
        position: LogicalPosition::new(r.0, r.1).into(),
        size: LogicalSize::new(r.2, r.3).into(),
    });
}

fn window_size(app: &AppHandle) -> Option<(f64, f64)> {
    let window = app.get_window("main")?;
    let scale = window.scale_factor().ok()?;
    let s = window.inner_size().ok()?.to_logical::<f64>(scale);
    Some((s.width, s.height))
}

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

fn layout_visible(app: &AppHandle) {
    let Some((w, h)) = window_size(app) else { return };
    if let Some(ui) = app.get_webview("ui") {
        place(&ui, (0.0, 0.0, w, h));
    }
    let tab_id = ACTIVE_TAB.load(Ordering::SeqCst);
    if tab_id != 0 {
        if let Some(wv) = app.get_webview(&format!("tab-{tab_id}")) {
            place(&wv, content_rect(w, h));
        }
    }
    let panel = ACTIVE_PANEL.lock().ok().and_then(|p| p.clone());
    if let Some(label) = panel {
        if let Some(wv) = app.get_webview(&label) {
            place(&wv, panel_rect(h));
        }
    }
}

fn hide_all_tabs(app: &AppHandle) {
    ACTIVE_TAB.store(0, Ordering::SeqCst);
    for (label, wv) in app.webviews() {
        if label.starts_with("tab-") {
            let _ = wv.hide();
        }
    }
}

fn hide_all_panels(app: &AppHandle) {
    if let Ok(mut p) = ACTIVE_PANEL.lock() {
        *p = None;
    }
    for (label, wv) in app.webviews() {
        if label.starts_with("panel-") {
            let _ = wv.hide();
        }
    }
}

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

fn watch_zen_edges(app: AppHandle) {
    std::thread::spawn(move || {
        let mut away_since: Option<Instant> = None;
        let mut sent_enter = false;
        let mut sent_exit = false;
        loop {
            let idle = !ZEN.load(Ordering::SeqCst) && !AUTO_HIDE_ENABLED.load(Ordering::SeqCst);
            std::thread::sleep(Duration::from_millis(if idle { 400 } else { 60 }));
            let Some(win) = app.get_window("main") else { return };
            if idle {
                away_since = None; sent_enter = false; sent_exit = false;
                continue;
            }
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

#[tauri::command]
async fn new_tab(app: AppHandle, url: Option<String>, private: Option<bool>) -> Result<u32, String> {
    let private = private.unwrap_or(false);
    let window = app.get_window("main").ok_or("fenêtre introuvable")?;
    let id = NEXT_ID.fetch_add(1, Ordering::SeqCst);
    let url = url.unwrap_or_default();
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
    let key_handle = app.clone();
    let dl_handle = app.clone();
    let current_host: std::sync::Arc<std::sync::Mutex<String>> =
        std::sync::Arc::new(std::sync::Mutex::new(String::new()));
    let host_for_load = current_host.clone();
    let mut builder = WebviewBuilder::new(format!("tab-{id}"), target)
        .initialization_script(page_script())
        .incognito(private)
        .on_document_title_changed(move |_wv, title| {
            if let Some(rest) = title.strip_prefix("\u{200b}tweax-key:") {
                let combo = rest.rsplit_once(':').map(|(c, _)| c).unwrap_or(rest);
                let _ = key_handle.emit("page-key", combo.to_string());
            }
        })
        .on_download(move |_wv, event| {
            match event {
                DownloadEvent::Requested { url, destination } => {
                    let name = destination
                        .file_name()
                        .map(|n| n.to_owned())
                        .unwrap_or_else(|| "telechargement".into());
                    let custom = DL_DIR.lock().ok().and_then(|d| d.clone()).filter(|d| d.is_dir());
                    if let Some(dir) = custom.or_else(|| dl_handle.path().download_dir().ok()) {
                        *destination = unique_path(&dir, &name);
                    }
                    if DL_ASK.load(Ordering::SeqCst) {
                        let mut dialog = rfd::FileDialog::new().set_file_name(name.to_string_lossy().to_string());
                        if let Some(dir) = destination.parent() {
                            dialog = dialog.set_directory(dir);
                        }
                        if let Some(win) = dl_handle.get_window("main") {
                            dialog = dialog.set_parent(&win);
                        }
                        match dialog.save_file() {
                            Some(chosen) => *destination = chosen,
                            None => return false,
                        }
                    }
                    let _ = dl_handle.emit(
                        "download-started",
                        DownloadInfo {
                            url: url.to_string(),
                            path: Some(destination.to_string_lossy().into_owned()),
                            success: None,
                        },
                    );
                }
                DownloadEvent::Finished { url, path, success } => {
                    let _ = dl_handle.emit(
                        "download-done",
                        DownloadInfo {
                            url: url.to_string(),
                            path: path.map(|p| p.to_string_lossy().into_owned()),
                            success: Some(success),
                        },
                    );
                }
                _ => {}
            }
            true
        })
        .on_new_window(move |url, _features| {
            let _ = popup_handle.emit("new-window", url.to_string());
            tauri::webview::NewWindowResponse::Deny
        })
        .transparent(cfg!(target_os = "windows"))
        .background_color(if is_home && cfg!(target_os = "windows") {
            tauri::webview::Color(0, 0, 0, 0)
        } else {
            tauri::webview::Color(255, 255, 255, 255)
        })
        .on_page_load(move |wv, payload| {
            if let Ok(mut h) = host_for_load.lock() {
                *h = payload.url().host_str().unwrap_or("").to_lowercase();
            }
            #[cfg(target_os = "windows")]
            {
                use tauri::webview::Color;
                if is_home_url(payload.url()) {
                    let _ = wv.set_background_color(Some(Color(0, 0, 0, 0)));
                } else {
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

    if !private {
        if let Some(dir) = profile_dir(&app) {
            builder = builder.data_directory(dir);
        }
    }

    let wv = window
        .add_child(builder, LogicalPosition::new(x, y), LogicalSize::new(cw, ch))
        .map_err(|e| e.to_string())?;
    adblock_native::install(&wv, current_host);
    if is_home {
        let _ = wv.hide();
    } else {
        ACTIVE_TAB.store(id, Ordering::SeqCst);
    }

    Ok(id)
}

#[tauri::command]
async fn switch_tab(app: AppHandle, id: u32) -> Result<(), String> {
    let wv = tab(&app, id)?;
    hide_all_tabs(&app);
    if let Some((w, h)) = window_size(&app) {
        place(&wv, content_rect(w, h));
    }
    wv.show().map_err(|e| e.to_string())?;
    ACTIVE_TAB.store(id, Ordering::SeqCst);
    let _ = wv.set_focus();
    Ok(())
}

#[tauri::command]
async fn set_adblock(app: AppHandle, enabled: bool, allow: Option<Vec<String>>) -> Result<(), String> {
    ADBLOCK.store(enabled, Ordering::SeqCst);
    let list: Vec<String> = allow
        .unwrap_or_default()
        .into_iter()
        .map(|h| h.trim().to_lowercase())
        .filter(|h| !h.is_empty() && h.len() <= 120 && h.chars().all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-'))
        .take(200)
        .collect();
    let script = format!(
        "window.__tweaxAllow = {};window.__tweaxAdSync && window.__tweaxAdSync();",
        serde_json::to_string(&list).map_err(|e| e.to_string())?
    );
    if let Ok(mut a) = ALLOW.lock() {
        *a = list;
    }
    for (label, wv) in app.webviews() {
        if label.starts_with("tab-") {
            let _ = wv.eval(&script);
        }
    }
    Ok(())
}

#[tauri::command]
async fn set_profile(id: String) -> Result<(), String> {
    if !valid_id(&id) {
        return Err("profil invalide".into());
    }
    if let Ok(mut p) = PROFILE.lock() {
        *p = id;
    }
    Ok(())
}

#[tauri::command]
async fn delete_profile_data(app: AppHandle, id: String) -> Result<(), String> {
    if !valid_id(&id) || id == "default" || id == profile_id() {
        return Err("profil invalide".into());
    }
    if let Some(root) = browsing_root(&app) {
        let dir = root.join(&id);
        if dir.is_dir() {
            std::fs::remove_dir_all(dir).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

#[tauri::command]
async fn set_downloads(dir: Option<String>, ask: bool) -> Result<(), String> {
    let path = dir.map(|d| d.trim().to_string()).filter(|d| !d.is_empty()).map(PathBuf::from);
    if let Ok(mut d) = DL_DIR.lock() {
        *d = path;
    }
    DL_ASK.store(ask, Ordering::SeqCst);
    Ok(())
}

#[tauri::command]
async fn pick_folder(app: AppHandle) -> Result<Option<String>, String> {
    let mut dialog = rfd::AsyncFileDialog::new();
    if let Some(win) = app.get_window("main") {
        dialog = dialog.set_parent(&win);
    }
    Ok(dialog.pick_folder().await.map(|h| h.path().to_string_lossy().into_owned()))
}

#[tauri::command]
async fn save_text_file(app: AppHandle, name: String, content: String) -> Result<String, String> {
    let clean: String = name
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
        .collect();
    if clean.is_empty() || content.len() > 20_000_000 {
        return Err("fichier invalide".into());
    }
    let custom = DL_DIR.lock().ok().and_then(|d| d.clone()).filter(|d| d.is_dir());
    let dir = custom
        .or_else(|| app.path().download_dir().ok())
        .ok_or("dossier introuvable")?;
    let target = unique_path(&dir, std::ffi::OsStr::new(&clean));
    std::fs::write(&target, content).map_err(|e| e.to_string())?;
    Ok(target.to_string_lossy().into_owned())
}

#[tauri::command]
async fn clear_browsing_data(app: AppHandle) -> Result<(), String> {
    clear_data(&app)
}

#[tauri::command]
async fn set_clear_on_exit(app: AppHandle, enabled: bool) -> Result<(), String> {
    CLEAR_ON_EXIT.store(enabled, Ordering::SeqCst);
    let dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let marker = dir.join("clear_on_exit");
    if enabled {
        std::fs::write(marker, b"1").map_err(|e| e.to_string())
    } else if marker.exists() {
        std::fs::remove_file(marker).map_err(|e| e.to_string())
    } else {
        Ok(())
    }
}

#[tauri::command]
async fn set_hw_accel(app: AppHandle, enabled: bool) -> Result<(), String> {
    let dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let marker = dir.join("no_hw_accel");
    if enabled {
        if marker.exists() {
            std::fs::remove_file(marker).map_err(|e| e.to_string())
        } else {
            Ok(())
        }
    } else {
        std::fs::write(marker, b"1").map_err(|e| e.to_string())
    }
}

#[tauri::command]
async fn get_hw_accel(app: AppHandle) -> Result<bool, String> {
    let dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    Ok(!dir.join("no_hw_accel").exists())
}

#[tauri::command]
async fn reader_mode(app: AppHandle, id: u32) -> Result<(), String> {
    tab(&app, id)?
        .eval("window.__tweaxReader && window.__tweaxReader.toggle()")
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn set_shortcuts(app: AppHandle, keys: Vec<String>) -> Result<(), String> {
    let script = format!(
        "window.__tweaxKeys = {};",
        serde_json::to_string(&keys).map_err(|e| e.to_string())?
    );
    if let Ok(mut k) = KEYS.lock() {
        *k = keys;
    }
    for (label, wv) in app.webviews() {
        if label.starts_with("tab-") {
            let _ = wv.eval(&script);
        }
    }
    Ok(())
}

#[tauri::command]
async fn find_in_page(app: AppHandle, id: u32) -> Result<(), String> {
    tab(&app, id)?
        .eval("window.__tweaxFind && window.__tweaxFind.open()")
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn reveal_path(path: String) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("explorer")
            .arg(format!("/select,{}", path.replace('/', "\\")))
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open").args(["-R", &path]).spawn().map_err(|e| e.to_string())?;
    }
    #[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
    {
        let dir = Path::new(&path).parent().map(|p| p.to_path_buf()).unwrap_or_else(|| PathBuf::from("."));
        std::process::Command::new("xdg-open").arg(dir).spawn().map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
async fn open_path(path: String) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("cmd")
            .args(["/C", "start", "", &path])
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open").arg(&path).spawn().map_err(|e| e.to_string())?;
    }
    #[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
    {
        std::process::Command::new("xdg-open").arg(&path).spawn().map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
async fn hide_all_tabs_cmd(app: AppHandle) -> Result<(), String> {
    hide_all_tabs(&app);
    if let Some(ui) = app.get_webview("ui") {
        let _ = ui.set_focus();
    }
    Ok(())
}

#[tauri::command]
async fn hide_tabs_quiet(app: AppHandle) -> Result<(), String> {
    hide_all_tabs(&app);
    Ok(())
}

#[tauri::command]
async fn close_tab(app: AppHandle, id: u32) -> Result<(), String> {
    let _ = ACTIVE_TAB.compare_exchange(id, 0, Ordering::SeqCst, Ordering::SeqCst);
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
    if CLEAR_ON_EXIT.load(Ordering::SeqCst) {
        let _ = clear_data(&app);
    }
    app.exit(0);
    Ok(())
}

#[tauri::command]
async fn win_drag(app: AppHandle) -> Result<(), String> {
    main_window(&app)?.start_dragging().map_err(|e| e.to_string())
}

#[tauri::command]
async fn set_tint(app: AppHandle, dark: bool) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        use tauri::window::{Color, Effect, EffectsBuilder};
        let color = if dark { Color(0, 0, 0, 24) } else { Color(255, 255, 255, 150) };
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

#[tauri::command]
async fn hide_tabs(app: AppHandle) -> Result<(), String> {
    hide_all_tabs(&app);
    hide_all_panels(&app);
    if let Some(ui) = app.get_webview("ui") {
        let _ = ui.set_focus();
    }
    Ok(())
}

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
                return;
            }
            let t = (start.elapsed().as_secs_f64() * 1000.0 / ZEN_MS).min(1.0);
            let e = 1.0 - (1.0 - t).powi(3);
            RAIL_CUR.store((from_r + (to_r - from_r) * e).to_bits(), Ordering::SeqCst);
            TOP_CUR.store((from_t + (to_t - from_t) * e).to_bits(), Ordering::SeqCst);
            if t >= 1.0 {
                layout(&app);
                break;
            }
            layout_visible(&app);
            std::thread::sleep(Duration::from_millis(12));
        }
    });
    Ok(())
}

#[tauri::command]
async fn set_panel(app: AppHandle, open: bool) -> Result<(), String> {
    PANEL_OPEN.store(open, Ordering::SeqCst);
    if !open {
        hide_all_panels(&app);
    }
    layout_visible(&app);
    Ok(())
}

#[tauri::command]
async fn show_web_panel(app: AppHandle, key: String, url: String) -> Result<(), String> {
    let window = app.get_window("main").ok_or("fenêtre introuvable")?;
    let label = format!("panel-{key}");
    let (_, h) = window_size(&app).ok_or("taille introuvable")?;

    hide_all_panels(&app);
    PANEL_OPEN.store(true, Ordering::SeqCst);

    if let Ok(mut p) = ACTIVE_PANEL.lock() {
        *p = Some(label.clone());
    }

    if let Some(wv) = app.get_webview(&label) {
        wv.show().map_err(|e| e.to_string())?;
    } else {
        let r = panel_rect(h);
        window
            .add_child(
                {
                    let mut b = WebviewBuilder::new(label, WebviewUrl::External(normalize(&url)));
                    if let Some(dir) = profile_dir(&app) {
                        b = b.data_directory(dir);
                    }
                    b
                },
                LogicalPosition::new(r.0, r.1),
                LogicalSize::new(r.2, r.3),
            )
            .map_err(|e| e.to_string())?;
    }
    layout_visible(&app);
    Ok(())
}

#[tauri::command]
async fn hide_web_panels(app: AppHandle) -> Result<(), String> {
    hide_all_panels(&app);
    Ok(())
}

#[tauri::command]
async fn close_web_panel(app: AppHandle, key: String) -> Result<(), String> {
    let label = format!("panel-{key}");
    if let Ok(mut p) = ACTIVE_PANEL.lock() {
        if p.as_deref() == Some(label.as_str()) {
            *p = None;
        }
    }
    if let Some(wv) = app.get_webview(&label) {
        wv.close().map_err(|e| e.to_string())?;
    }
    Ok(())
}

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

#[tauri::command]
async fn vault_import_preview(state: State<'_, VaultState>, csv: String) -> Result<vault::Preview, String> {
    with_vault(&state, |v| v.import_preview(&csv))
}

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

#[tauri::command]
async fn vault_matches(state: State<'_, VaultState>, host: String) -> Result<Vec<vault::PublicEntry>, String> {
    with_vault(&state, |v| v.matches_host(&host))
}

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
            #[cfg(desktop)]
            {
                if app.config().plugins.0.contains_key("updater") {
                    app.handle().plugin(tauri_plugin_updater::Builder::new().build())?;
                }
            }

            let dir = app.path().app_data_dir()?;
            std::fs::create_dir_all(&dir)?;
            #[cfg(target_os = "windows")]
            {
                if dir.join("no_hw_accel").exists() {
                    let extra = "--disable-gpu --disable-gpu-compositing";
                    let args = match std::env::var("WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS") {
                        Ok(cur) if !cur.trim().is_empty() => format!("{cur} {extra}"),
                        _ => extra.to_string(),
                    };
                    std::env::set_var("WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS", args);
                }
            }
            if dir.join("clear_on_exit").exists() {
                CLEAR_ON_EXIT.store(true, Ordering::SeqCst);
                if let Some(root) = browsing_root(app.handle()) {
                    let _ = std::fs::remove_dir_all(root);
                }
            }
            app.manage(VaultState(Mutex::new(vault::Vault::new(dir.join("vault.bin")))));

            let mut builder = WindowBuilder::new(app, "main")
                .title("Tweax")
                .inner_size(1280.0, 820.0)
                .min_inner_size(760.0, 420.0)
                .decorations(false)
                .shadow(true)
                .transparent(cfg!(target_os = "windows"));
            if let Some(icon) = app.default_window_icon() {
                builder = builder.icon(icon.clone())?;
            }
            let window = builder.build()?;

            let size = window
                .inner_size()?
                .to_logical::<f64>(window.scale_factor()?);

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
            match event {
                WindowEvent::Resized(_) => layout_visible(window.app_handle()),
                WindowEvent::CloseRequested { .. } => {
                    if CLEAR_ON_EXIT.load(Ordering::SeqCst) {
                        let _ = clear_data(window.app_handle());
                    }
                }
                _ => {}
            }
        })
        .invoke_handler(tauri::generate_handler![
            new_tab,
            switch_tab,
            hide_all_tabs_cmd,
            hide_tabs_quiet,
            set_adblock,
            set_profile,
            delete_profile_data,
            set_downloads,
            pick_folder,
            save_text_file,
            clear_browsing_data,
            set_clear_on_exit,
            set_hw_accel,
            get_hw_accel,
            reader_mode,
            set_shortcuts,
            find_in_page,
            reveal_path,
            open_path,
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
