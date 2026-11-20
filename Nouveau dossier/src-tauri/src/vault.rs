use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use argon2::{Algorithm, Argon2, Params, Version};
use chacha20poly1305::{
    aead::{Aead, KeyInit, Payload},
    XChaCha20Poly1305, XNonce,
};
use rand::{rngs::OsRng, RngCore};
use serde::{Deserialize, Serialize};
use zeroize::{Zeroize, ZeroizeOnDrop, Zeroizing};

const FILE_VERSION: u32 = 1;
const AAD: &[u8] = b"tweax-vault-v1";
const MIN_MASTER_LEN: usize = 8;
const KDF_M_KIB: u32 = 64 * 1024;
const KDF_T: u32 = 3;
const KDF_P: u32 = 1;

#[derive(Clone, Serialize, Deserialize, Zeroize, ZeroizeOnDrop)]
pub struct Entry {
    pub id: String,
    pub name: String,
    pub url: String,
    pub username: String,
    pub password: String,
    pub note: String,
    pub created: u64,
}

#[derive(Serialize)]
pub struct PublicEntry {
    pub id: String,
    pub name: String,
    pub url: String,
    pub username: String,
    pub created: u64,
}

#[derive(Serialize)]
pub struct Status {
    pub exists: bool,
    pub unlocked: bool,
    pub count: usize,
}

#[derive(Serialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "lowercase")]
pub enum RowStatus {
    New,
    Duplicate,
    Conflict,
}

#[derive(Serialize)]
pub struct PreviewRow {
    pub name: String,
    pub url: String,
    pub username: String,
    pub status: RowStatus,
}

#[derive(Serialize)]
pub struct Preview {
    pub format: String,
    pub rows: Vec<PreviewRow>,
    pub new: usize,
    pub duplicates: usize,
    pub conflicts: usize,
    pub invalid: usize,
}

#[derive(Serialize, Debug)]
pub struct ImportResult {
    pub added: usize,
    pub updated: usize,
    pub skipped: usize,
}

#[derive(Serialize)]
pub struct AuditItem {
    pub id: String,
    pub reason: &'static str,
}

fn is_weak(p: &str) -> bool {
    let len = p.chars().count();
    let classes = [
        p.chars().any(|c| c.is_lowercase()),
        p.chars().any(|c| c.is_uppercase()),
        p.chars().any(|c| c.is_ascii_digit()),
        p.chars().any(|c| !c.is_alphanumeric()),
    ]
    .iter()
    .filter(|b| **b)
    .count();
    const COMMON: [&str; 8] = ["password", "123456", "12345678", "qwerty", "azerty", "motdepasse", "letmein", "admin"];
    let low = p.to_lowercase();
    len < 8 || (len < 12 && classes < 3) || (len < 16 && classes < 2) || COMMON.iter().any(|c| low.contains(c))
}

#[derive(Serialize, Deserialize)]
struct VaultFile {
    v: u32,
    m_kib: u32,
    t: u32,
    p: u32,
    salt: String,
    nonce: String,
    data: String,
}

struct Unlocked {
    key: Zeroizing<[u8; 32]>,
    salt: [u8; 16],
    params: (u32, u32, u32),
    entries: Vec<Entry>,
}

struct Pending {
    items: Vec<(Entry, RowStatus)>,
}

pub struct Vault {
    path: PathBuf,
    state: Option<Unlocked>,
    pending: Option<Pending>,
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

fn unhex(s: &str) -> Result<Vec<u8>, String> {
    if s.len() % 2 != 0 {
        return Err("fichier du coffre corrompu".into());
    }
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).map_err(|_| "fichier du coffre corrompu".to_string()))
        .collect()
}

fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

fn new_id() -> String {
    let mut b = [0u8; 8];
    OsRng.fill_bytes(&mut b);
    hex(&b)
}

fn derive_key(master: &str, salt: &[u8], (m, t, p): (u32, u32, u32)) -> Result<Zeroizing<[u8; 32]>, String> {
    let params = Params::new(m, t, p, Some(32)).map_err(|e| e.to_string())?;
    let mut key = Zeroizing::new([0u8; 32]);
    Argon2::new(Algorithm::Argon2id, Version::V0x13, params)
        .hash_password_into(master.as_bytes(), salt, key.as_mut())
        .map_err(|e| e.to_string())?;
    Ok(key)
}

pub fn site_key(url: &str) -> String {
    let u = url.trim();
    let with_scheme = if u.contains("://") { u.to_string() } else { format!("https://{u}") };
    match host_of(&with_scheme) {
        Some(h) => h,
        None => u.to_lowercase(),
    }
}

fn host_of(u: &str) -> Option<String> {
    let rest = u.split_once("://")?.1;
    let auth = rest.split(['/', '?', '#']).next()?;
    let host = auth.rsplit('@').next()?;
    let host = host.split(':').next()?.to_lowercase();
    if host.is_empty() {
        return None;
    }
    Some(host.strip_prefix("www.").map(str::to_string).unwrap_or(host))
}

struct ParsedRow {
    name: String,
    url: String,
    username: String,
    password: String,
    note: String,
}

struct Parsed {
    format: String,
    rows: Vec<ParsedRow>,
    invalid: usize,
}

fn detect_delimiter(first_line: &str) -> u8 {
    [b',', b';', b'\t']
        .into_iter()
        .max_by_key(|d| first_line.bytes().filter(|b| b == d).count())
        .unwrap_or(b',')
}

fn find_col(headers: &[String], names: &[&str]) -> Option<usize> {
    names.iter().find_map(|n| headers.iter().position(|h| h == n))
}

fn parse_csv(text: &str) -> Result<Parsed, String> {
    let text = text.trim_start_matches('\u{feff}');
    let first = text.lines().next().unwrap_or("");
    if first.trim().is_empty() {
        return Err("fichier vide".into());
    }
    let mut rdr = csv::ReaderBuilder::new()
        .delimiter(detect_delimiter(first))
        .flexible(true)
        .trim(csv::Trim::None)
        .from_reader(text.as_bytes());

    let headers: Vec<String> = rdr
        .headers()
        .map_err(|e| format!("CSV illisible : {e}"))?
        .iter()
        .map(|h| h.trim().trim_start_matches('\u{feff}').to_lowercase())
        .collect();

    let c_pass = find_col(&headers, &["password", "login_password", "mot de passe", "pass"]);
    let c_url = find_col(&headers, &["url", "login_uri", "uri", "website", "web site", "site", "origin", "hostname", "address"]);
    let c_user = find_col(&headers, &["username", "login_username", "user name", "user", "email", "login", "identifiant"]);
    let c_name = find_col(&headers, &["name", "title", "titre", "nom"]);
    let c_note = find_col(&headers, &["note", "notes", "extra", "comment"]);

    let Some(c_pass) = c_pass else {
        return Err("colonne « password » introuvable : ce n'est pas un export de mots de passe".into());
    };
    if c_url.is_none() && c_name.is_none() {
        return Err("aucune colonne « url » ou « name » : format non reconnu".into());
    }

    let has = |n: &str| headers.iter().any(|h| h == n);
    let format = if has("httprealm") || has("guid") || has("formactionorigin") {
        "Firefox"
    } else if has("otpauth") {
        "Safari"
    } else if has("login_uri") || has("login_username") {
        "Bitwarden"
    } else if has("name") && has("url") && has("username") {
        "Chrome / Edge / Brave / Opera"
    } else {
        "CSV générique"
    }
    .to_string();

    let get = |rec: &csv::StringRecord, c: Option<usize>| -> String {
        c.and_then(|i| rec.get(i)).unwrap_or("").trim().to_string()
    };

    let mut rows = Vec::new();
    let mut invalid = 0;
    for rec in rdr.records() {
        let Ok(rec) = rec else {
            invalid += 1;
            continue;
        };
        if rec.iter().all(|f| f.trim().is_empty()) {
            continue;
        }
        let password = rec.get(c_pass).unwrap_or("").to_string();
        let mut url = get(&rec, c_url);
        let mut name = get(&rec, c_name);
        let username = get(&rec, c_user);
        let note = get(&rec, c_note);

        let internal = ["chrome://", "about:", "moz-extension://", "chrome-extension://"]
            .iter()
            .any(|p| url.to_lowercase().starts_with(p));
        if password.is_empty() || internal || (url.is_empty() && name.is_empty()) {
            invalid += 1;
            continue;
        }
        if url.is_empty() {
            url = name.clone();
        }
        if name.is_empty() {
            name = host_of(&if url.contains("://") { url.clone() } else { format!("https://{url}") })
                .unwrap_or_else(|| url.clone());
        }
        rows.push(ParsedRow { name, url, username, password, note });
    }
    Ok(Parsed { format, rows, invalid })
}

impl Vault {
    pub fn new(path: PathBuf) -> Self {
        Self { path, state: None, pending: None }
    }

    pub fn exists(&self) -> bool {
        self.path.exists()
    }

    pub fn status(&self) -> Status {
        Status {
            exists: self.exists(),
            unlocked: self.state.is_some(),
            count: self.state.as_ref().map_or(0, |s| s.entries.len()),
        }
    }

    fn unlocked(&mut self) -> Result<&mut Unlocked, String> {
        self.state.as_mut().ok_or_else(|| "coffre verrouillé".to_string())
    }

    pub fn create(&mut self, master: &str) -> Result<(), String> {
        if self.exists() {
            return Err("un coffre existe déjà".into());
        }
        if master.chars().count() < MIN_MASTER_LEN {
            return Err(format!("mot de passe maître trop court ({MIN_MASTER_LEN} caractères minimum)"));
        }
        let mut salt = [0u8; 16];
        OsRng.fill_bytes(&mut salt);
        let params = (KDF_M_KIB, KDF_T, KDF_P);
        let key = derive_key(master, &salt, params)?;
        let st = Unlocked { key, salt, params, entries: Vec::new() };
        save(&self.path, &st)?;
        self.state = Some(st);
        Ok(())
    }

    pub fn unlock(&mut self, master: &str) -> Result<(), String> {
        let raw = fs::read_to_string(&self.path).map_err(|_| "aucun coffre".to_string())?;
        let f: VaultFile = serde_json::from_str(&raw).map_err(|_| "fichier du coffre corrompu".to_string())?;
        if f.v != FILE_VERSION {
            return Err("version de coffre non prise en charge".into());
        }
        let salt_v = unhex(&f.salt)?;
        let salt: [u8; 16] = salt_v.try_into().map_err(|_| "fichier du coffre corrompu".to_string())?;
        let params = (f.m_kib, f.t, f.p);
        let key = derive_key(master, &salt, params)?;
        let nonce = unhex(&f.nonce)?;
        if nonce.len() != 24 {
            return Err("fichier du coffre corrompu".into());
        }
        let ct = unhex(&f.data)?;
        let cipher = XChaCha20Poly1305::new(key.as_ref().into());
        let plain = Zeroizing::new(
            cipher
                .decrypt(XNonce::from_slice(&nonce), Payload { msg: &ct, aad: AAD })
                .map_err(|_| "mot de passe maître incorrect".to_string())?,
        );
        let entries: Vec<Entry> = serde_json::from_slice(&plain).map_err(|_| "contenu du coffre illisible".to_string())?;
        self.state = Some(Unlocked { key, salt, params, entries });
        Ok(())
    }

    pub fn lock(&mut self) {
        self.state = None;
        self.pending = None;
    }

    pub fn list(&mut self) -> Result<Vec<PublicEntry>, String> {
        let mut v: Vec<PublicEntry> = self
            .unlocked()?
            .entries
            .iter()
            .map(|e| PublicEntry { id: e.id.clone(), name: e.name.clone(), url: e.url.clone(), username: e.username.clone(), created: e.created })
            .collect();
        v.sort_by_key(|e| e.name.to_lowercase());
        Ok(v)
    }

    pub fn reveal(&mut self, id: &str) -> Result<String, String> {
        self.unlocked()?
            .entries
            .iter()
            .find(|e| e.id == id)
            .map(|e| e.password.clone())
            .ok_or_else(|| "entrée introuvable".to_string())
    }

    pub fn delete(&mut self, id: &str) -> Result<(), String> {
        let path = self.path.clone();
        let st = self.unlocked()?;
        let before = st.entries.len();
        st.entries.retain(|e| e.id != id);
        if st.entries.len() == before {
            return Err("entrée introuvable".into());
        }
        save(&path, st)
    }

    pub fn save_entry(
        &mut self,
        id: Option<String>,
        name: &str,
        url: &str,
        username: &str,
        password: &str,
        note: Option<String>,
    ) -> Result<String, String> {
        let (name, url) = (name.trim(), url.trim());
        if password.is_empty() {
            return Err("le mot de passe est vide".into());
        }
        if name.is_empty() && url.is_empty() {
            return Err("indique un nom ou l'adresse du site".into());
        }
        let url = if url.is_empty() { name.to_string() } else { url.to_string() };
        let name = if name.is_empty() {
            host_of(&if url.contains("://") { url.clone() } else { format!("https://{url}") }).unwrap_or_else(|| url.clone())
        } else {
            name.to_string()
        };
        let path = self.path.clone();
        let st = self.unlocked()?;
        let out = if let Some(id) = id {
            let e = st.entries.iter_mut().find(|e| e.id == id).ok_or_else(|| "entrée introuvable".to_string())?;
            e.name = name;
            e.url = url;
            e.username = username.trim().to_string();
            e.password = password.to_string();
            if let Some(n) = note {
                e.note = n;
            }
            id
        } else {
            let id = new_id();
            st.entries.push(Entry {
                id: id.clone(),
                name,
                url,
                username: username.trim().to_string(),
                password: password.to_string(),
                note: note.unwrap_or_default(),
                created: now(),
            });
            id
        };
        save(&path, st)?;
        Ok(out)
    }

    pub fn change_master(&mut self, old: &str, new: &str) -> Result<(), String> {
        if new.chars().count() < MIN_MASTER_LEN {
            return Err(format!("nouveau mot de passe trop court ({MIN_MASTER_LEN} caractères minimum)"));
        }
        let path = self.path.clone();
        let st = self.unlocked()?;
        let check = derive_key(old, &st.salt, st.params)?;
        if *check != *st.key {
            return Err("mot de passe maître actuel incorrect".into());
        }
        let mut salt = [0u8; 16];
        OsRng.fill_bytes(&mut salt);
        let params = (KDF_M_KIB, KDF_T, KDF_P);
        st.key = derive_key(new, &salt, params)?;
        st.salt = salt;
        st.params = params;
        save(&path, st)
    }

    pub fn matches_host(&mut self, host: &str) -> Result<Vec<PublicEntry>, String> {
        let h = site_key(host);
        Ok(self
            .unlocked()?
            .entries
            .iter()
            .filter(|e| site_key(&e.url) == h)
            .map(|e| PublicEntry { id: e.id.clone(), name: e.name.clone(), url: e.url.clone(), username: e.username.clone(), created: e.created })
            .collect())
    }

    pub fn credentials(&mut self, id: &str) -> Result<(String, String, String), String> {
        self.unlocked()?
            .entries
            .iter()
            .find(|e| e.id == id)
            .map(|e| (e.url.clone(), e.username.clone(), e.password.clone()))
            .ok_or_else(|| "entrée introuvable".to_string())
    }

    pub fn audit(&mut self) -> Result<Vec<AuditItem>, String> {
        let entries = &self.unlocked()?.entries;
        let mut out = Vec::new();
        for e in entries {
            if is_weak(&e.password) {
                out.push(AuditItem { id: e.id.clone(), reason: "weak" });
            }
            let k = site_key(&e.url);
            if entries.iter().any(|o| o.id != e.id && o.password == e.password && site_key(&o.url) != k) {
                out.push(AuditItem { id: e.id.clone(), reason: "reused" });
            }
        }
        Ok(out)
    }

    pub fn import_preview(&mut self, csv_text: &str) -> Result<Preview, String> {
        self.unlocked()?;
        let parsed = parse_csv(csv_text)?;

        let mut seen: Vec<(String, String, String)> = self
            .unlocked()?
            .entries
            .iter()
            .map(|e| (site_key(&e.url), e.username.to_lowercase(), e.password.clone()))
            .collect();

        let (mut n, mut d, mut c) = (0, 0, 0);
        let mut rows = Vec::new();
        let mut items = Vec::new();
        for r in parsed.rows {
            let key = site_key(&r.url);
            let user = r.username.to_lowercase();
            let status = if seen.iter().any(|(k, u, p)| *k == key && *u == user && *p == r.password) {
                d += 1;
                RowStatus::Duplicate
            } else if seen.iter().any(|(k, u, _)| *k == key && *u == user) {
                c += 1;
                RowStatus::Conflict
            } else {
                n += 1;
                RowStatus::New
            };
            seen.push((key, user, r.password.clone()));
            rows.push(PreviewRow { name: r.name.clone(), url: r.url.clone(), username: r.username.clone(), status });
            items.push((
                Entry { id: new_id(), name: r.name, url: r.url, username: r.username, password: r.password, note: r.note, created: now() },
                status,
            ));
        }
        self.pending = Some(Pending { items });
        Ok(Preview { format: parsed.format, rows, new: n, duplicates: d, conflicts: c, invalid: parsed.invalid })
    }

    pub fn import_cancel(&mut self) {
        self.pending = None;
    }

    pub fn import_commit(&mut self, replace_conflicts: bool) -> Result<ImportResult, String> {
        let pending = self.pending.take().ok_or_else(|| "aucun import en attente".to_string())?;
        let path = self.path.clone();
        let st = self.unlocked()?;
        let (mut added, mut updated, mut skipped) = (0, 0, 0);
        for (entry, status) in pending.items.iter() {
            match status {
                RowStatus::New => {
                    st.entries.push(entry.clone());
                    added += 1;
                }
                RowStatus::Duplicate => skipped += 1,
                RowStatus::Conflict => {
                    if replace_conflicts {
                        let key = site_key(&entry.url);
                        let user = entry.username.to_lowercase();
                        if let Some(ex) = st
                            .entries
                            .iter_mut()
                            .find(|e| site_key(&e.url) == key && e.username.to_lowercase() == user)
                        {
                            ex.password = entry.password.clone();
                            if !entry.note.is_empty() {
                                ex.note = entry.note.clone();
                            }
                            updated += 1;
                        }
                    } else {
                        skipped += 1;
                    }
                }
            }
        }
        save(&path, st)?;
        Ok(ImportResult { added, updated, skipped })
    }
}

fn save(path: &Path, st: &Unlocked) -> Result<(), String> {
    let plain = Zeroizing::new(serde_json::to_vec(&st.entries).map_err(|e| e.to_string())?);
    let mut nonce = [0u8; 24];
    OsRng.fill_bytes(&mut nonce);
    let cipher = XChaCha20Poly1305::new(st.key.as_ref().into());
    let ct = cipher
        .encrypt(XNonce::from_slice(&nonce), Payload { msg: &plain, aad: AAD })
        .map_err(|_| "chiffrement impossible".to_string())?;
    let file = VaultFile {
        v: FILE_VERSION,
        m_kib: st.params.0,
        t: st.params.1,
        p: st.params.2,
        salt: hex(&st.salt),
        nonce: hex(&nonce),
        data: hex(&ct),
    };
    let json = serde_json::to_vec(&file).map_err(|e| e.to_string())?;
    let tmp = path.with_extension("tmp");
    {
        let mut f = fs::File::create(&tmp).map_err(|e| e.to_string())?;
        f.write_all(&json).map_err(|e| e.to_string())?;
        f.sync_all().map_err(|e| e.to_string())?;
    }
    fs::rename(&tmp, path).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> PathBuf {
        let mut p = std::env::temp_dir();
        p.push(format!("tweax-vault-test-{}-{}", std::process::id(), name));
        let _ = fs::remove_file(&p);
        p
    }

    const CHROME: &str = "name,url,username,password,note\n\
        GitHub,https://github.com/login,alice,pa$$,\"note, avec virgule\"\n\
        Example,https://www.example.com/,bob,\"mdp\"\"guillemet\",\n\
        Vide,https://vide.test/,carl,,\n\
        Interne,chrome://settings,x,y,\n";

    #[test]
    fn chrome_parse() {
        let p = parse_csv(CHROME).unwrap();
        assert_eq!(p.format, "Chrome / Edge / Brave / Opera");
        assert_eq!(p.rows.len(), 2);
        assert_eq!(p.invalid, 2);
        assert_eq!(p.rows[0].note, "note, avec virgule");
        assert_eq!(p.rows[1].password, "mdp\"guillemet");
    }

    #[test]
    fn firefox_safari_bitwarden_semicolon_bom() {
        let ff = "url,username,password,httpRealm,formActionOrigin,guid,timeCreated\nhttps://a.com,u,p1,,https://a.com,{x},1\n";
        let p = parse_csv(ff).unwrap();
        assert_eq!((p.format.as_str(), p.rows.len()), ("Firefox", 1));
        assert_eq!(p.rows[0].name, "a.com");

        let sf = "Title,URL,Username,Password,Notes,OTPAuth\nMon site,https://b.com,u,p2,n,\n";
        assert_eq!(parse_csv(sf).unwrap().format, "Safari");

        let bw = "folder,favorite,type,name,notes,fields,reprompt,login_uri,login_username,login_password,login_totp\n,,login,B,,,0,https://c.com,u,p3,\n";
        let p = parse_csv(bw).unwrap();
        assert_eq!((p.format.as_str(), p.rows.len()), ("Bitwarden", 1));
        assert_eq!(p.rows[0].password, "p3");

        let semi = "\u{feff}name;url;username;password\nD;https://d.com;u;p4\n";
        let p = parse_csv(semi).unwrap();
        assert_eq!(p.rows.len(), 1);
        assert_eq!(p.rows[0].password, "p4");
    }

    #[test]
    fn rejects_non_password_csv() {
        assert!(parse_csv("a,b\n1,2\n").is_err());
        assert!(parse_csv("").is_err());
    }

    #[test]
    fn password_whitespace_preserved() {
        let p = parse_csv("name,url,username,password\nA,https://a.com,u, secret \n").unwrap();
        assert_eq!(p.rows[0].password, " secret ");
    }

    #[test]
    fn site_keys() {
        assert_eq!(site_key("https://WWW.Example.com:8443/a?b#c"), "example.com");
        assert_eq!(site_key("example.com/login"), "example.com");
        assert_eq!(site_key("android://hash@com.app/"), "com.app");
    }

    #[test]
    fn full_flow_encrypted_on_disk() {
        let path = tmp("flow");
        let mut v = Vault::new(path.clone());
        assert!(v.create("court").is_err());
        v.create("un-bon-mot-de-passe").unwrap();
        assert!(v.create("un-bon-mot-de-passe").is_err());

        let pv = v.import_preview(CHROME).unwrap();
        assert_eq!((pv.new, pv.duplicates, pv.conflicts, pv.invalid), (2, 0, 0, 2));
        let r = v.import_commit(false).unwrap();
        assert_eq!((r.added, r.updated, r.skipped), (2, 0, 0));

        let raw = fs::read_to_string(&path).unwrap();
        for leak in ["alice", "github", "pa$$", "example.com"] {
            assert!(!raw.contains(leak), "fuite: {leak}");
        }

        let again = "name,url,username,password\nGitHub,https://github.com/login,alice,pa$$\nGitHub2,https://github.com/,ALICE,nouveau\nNeuf,https://neuf.fr,z,zz\n";
        let pv = v.import_preview(again).unwrap();
        assert_eq!((pv.new, pv.duplicates, pv.conflicts), (1, 1, 1));
        let r = v.import_commit(true).unwrap();
        assert_eq!((r.added, r.updated, r.skipped), (1, 1, 1));

        let list = v.list().unwrap();
        assert_eq!(list.len(), 3);
        let gh = list.iter().find(|e| e.url.contains("github")).unwrap();
        assert_eq!(v.reveal(&gh.id).unwrap(), "nouveau");

        v.lock();
        assert!(v.list().is_err());
        assert!(v.unlock("mauvais-mot-de-passe").is_err());
        v.unlock("un-bon-mot-de-passe").unwrap();
        assert_eq!(v.list().unwrap().len(), 3);

        let id = v.list().unwrap()[0].id.clone();
        v.delete(&id).unwrap();
        v.lock();
        v.unlock("un-bon-mot-de-passe").unwrap();
        assert_eq!(v.list().unwrap().len(), 2);

        v.lock();
        assert!(v.import_preview(CHROME).is_err());
        let _ = fs::remove_file(path);
    }

    #[test]
    fn edit_master_audit_matches() {
        let path = tmp("extra");
        let mut v = Vault::new(path.clone());
        v.create("un-bon-mot-de-passe").unwrap();
        let a = v.save_entry(None, "", "https://www.shop.fr/login", " me ", "Tr0ub4dor&3-long-enough", Some("n".into())).unwrap();
        let b = v.save_entry(None, "Autre", "https://autre.com", "me", "Tr0ub4dor&3-long-enough", None).unwrap();
        let c = v.save_entry(None, "Faible", "https://faible.com", "me", "abc", None).unwrap();
        assert!(v.save_entry(None, "x", "", "", "", None).is_err());
        assert!(v.save_entry(None, "", "", "", "pw", None).is_err());
        assert_eq!(v.list().unwrap().iter().find(|e| e.id == a).unwrap().name, "shop.fr");
        assert_eq!(v.list().unwrap().iter().find(|e| e.id == a).unwrap().username, "me");

        v.save_entry(Some(a.clone()), "Shop", "https://shop.fr", "me", "nouveau-Mdp-9!xyz", None).unwrap();
        assert_eq!(v.reveal(&a).unwrap(), "nouveau-Mdp-9!xyz");
        assert_eq!(v.unlocked().unwrap().entries.iter().find(|e| e.id == a).unwrap().note, "n");

        let au = v.audit().unwrap();
        assert!(au.iter().any(|i| i.id == c && i.reason == "weak"));
        assert!(!au.iter().any(|i| i.id == b));
        v.save_entry(Some(a.clone()), "Shop", "https://shop.fr", "me", "Tr0ub4dor&3-long-enough", None).unwrap();
        let au = v.audit().unwrap();
        assert!(au.iter().any(|i| i.id == a && i.reason == "reused") && au.iter().any(|i| i.id == b && i.reason == "reused"));

        assert_eq!(v.matches_host("shop.fr").unwrap().len(), 1);
        assert_eq!(v.matches_host("www.SHOP.fr").unwrap().len(), 1);
        assert_eq!(v.matches_host("evil.shop.fr").unwrap().len(), 0);
        assert_eq!(v.matches_host("shop.fr.evil.com").unwrap().len(), 0);

        assert!(v.change_master("mauvais", "nouveau-master-123").is_err());
        assert!(v.change_master("un-bon-mot-de-passe", "court").is_err());
        v.change_master("un-bon-mot-de-passe", "nouveau-master-123").unwrap();
        v.lock();
        assert!(v.unlock("un-bon-mot-de-passe").is_err());
        v.unlock("nouveau-master-123").unwrap();
        assert_eq!(v.list().unwrap().len(), 3);
        let _ = fs::remove_file(path);
    }

    #[test]
    fn tampering_is_detected() {
        let path = tmp("tamper");
        let mut v = Vault::new(path.clone());
        v.create("un-bon-mot-de-passe").unwrap();
        v.lock();
        let mut f: VaultFile = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        let last = f.data.pop().unwrap();
        f.data.push(if last == '0' { '1' } else { '0' });
        fs::write(&path, serde_json::to_vec(&f).unwrap()).unwrap();
        assert!(v.unlock("un-bon-mot-de-passe").is_err());
        let _ = fs::remove_file(path);
    }
}
