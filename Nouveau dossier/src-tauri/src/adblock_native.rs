#![cfg_attr(not(windows), allow(dead_code))]

use std::borrow::Cow;
use std::collections::HashSet;
use std::sync::OnceLock;

const BLOCK: &[&str] = &[
    "doubleclick.net", "googlesyndication.com", "googleadservices.com", "google-analytics.com",
    "adservice.google.com", "adservice.google.fr", "adservice.google.de", "pagead2.googlesyndication.com",
    "pagead.l.doubleclick.net", "partnerad.l.doubleclick.net", "googleads.g.doubleclick.net",
    "static.doubleclick.net", "tpc.googlesyndication.com", "s0.2mdn.net",
    "fundingchoicesmessages.google.com", "googletagservices.com", "adnxs.com", "adnxs-simple.com",
    "adsrvr.org", "taboola.com", "outbrain.com", "criteo.com", "criteo.net", "rubiconproject.com",
    "pubmatic.com", "openx.net", "openx.com", "casalemedia.com", "amazon-adsystem.com",
    "scorecardresearch.com", "quantserve.com", "moatads.com", "adsafeprotected.com", "advertising.com",
    "2mdn.net", "yieldmo.com", "smartadserver.com", "teads.tv", "media.net", "bidswitch.net",
    "contextweb.com", "lijit.com", "sharethrough.com", "indexww.com", "zedo.com", "serving-sys.com",
    "hotjar.com", "mouseflow.com", "krxd.net", "demdex.net", "bluekai.com", "adform.net", "mathtag.com",
    "rlcdn.com", "tapad.com", "33across.com", "sitescout.com", "exelator.com", "everesttech.net",
    "adcolony.com", "adroll.com", "popads.net", "popcash.net", "propellerads.com", "exoclick.com",
    "juicyads.com", "trafficjunky.net", "revcontent.com", "mgid.com", "ads.yahoo.com", "adtechus.com",
    "adtech.de", "adap.tv", "adbutler.com", "adblade.com", "adsterra.com", "bidvertiser.com",
    "infolinks.com", "chitika.com", "sovrn.com", "springserve.com", "spotxchange.com", "spotx.tv",
    "connatix.com", "tribalfusion.com", "yieldlab.net", "smaato.net", "smaato.com", "inmobi.com",
    "mopub.com", "vungle.com", "applovin.com", "unityads.unity3d.com", "chartboost.com", "ironsrc.com",
    "startapp.com", "appnexus.com", "undertone.com", "gumgum.com", "triplelift.com", "loopme.com",
    "freewheel.tv", "adsymptotic.com", "crwdcntrl.net", "eyeota.net", "agkn.com", "turn.com",
    "mediamath.com", "simpli.fi", "nativo.com", "dianomi.com", "zergnet.com", "content.ad",
    "exponential.com", "doubleverify.com", "serverbid.com", "rhythmone.com", "sonobi.com",
    "aniview.com", "innovid.com", "smartclip.net", "adingo.jp", "microad.jp",
    "bidtheatre.com", "adkernel.com", "districtm.io", "improvedigital.com", "adition.com",
    "adnami.io", "adswizz.com", "onetag-sys.com", "flashtalking.com", "narrative.io", "liveintent.com",
    "clicktale.net", "crazyegg.com", "fullstory.com", "mixpanel.com", "segment.io", "segment.com",
    "amplitude.com", "kissmetrics.io", "clicktripz.com", "adthrive.com", "mediavine.com",
    "ezoic.net", "monumetric.com", "raptive.com", "cafemedia.com", "venatus.com", "primis.tech",
    "gatekeeperconsent.com", "cookiebot.com", "quantcast.com", "permutive.com",
    "attributionapp.com", "branch.io", "adjust.com", "appsflyer.com", "kochava.com", "singular.net",
];

const PATH_MARKERS: &[&str] = &[
    "/pagead/", "/adserver/", "/adsystem/", "/advertisement/", "/openad", "/ad-iframe", "/adframe",
    "/admanager/", "/adslot", "/prebid/", "/api/stats/ads", "/get_midroll_info", "/ads/serve", "/adservice/",
    "/vast/", "/vpaid/", "/adtag/", "/adrequest", "/sponsorad/", "/promoted/",
];

fn block_set() -> &'static HashSet<&'static str> {
    static SET: OnceLock<HashSet<&'static str>> = OnceLock::new();
    SET.get_or_init(|| BLOCK.iter().copied().collect())
}

fn lower(s: &str) -> Cow<'_, str> {
    if s.bytes().any(|b| b.is_ascii_uppercase()) {
        Cow::Owned(s.to_ascii_lowercase())
    } else {
        Cow::Borrowed(s)
    }
}

pub fn host_of(uri: &str) -> Option<&str> {
    let without_scheme = uri.split_once("://").map(|(_, r)| r).unwrap_or(uri);
    let authority = without_scheme.split(['/', '?', '#']).next().unwrap_or("");
    let host = authority.rsplit_once('@').map(|(_, h)| h).unwrap_or(authority);
    let host = host.split(':').next().unwrap_or(host);
    if host.is_empty() { None } else { Some(host) }
}

pub fn path_of(uri: &str) -> &str {
    let without_scheme = uri.split_once("://").map(|(_, r)| r).unwrap_or(uri);
    match without_scheme.find('/') {
        Some(i) => &without_scheme[i..],
        None => "",
    }
}

fn suffix_match(host: &str, domain: &str) -> bool {
    host == domain
        || (host.len() > domain.len()
            && host.ends_with(domain)
            && host.as_bytes()[host.len() - domain.len() - 1] == b'.')
}

pub fn host_matches(host: &str, list: &[String]) -> bool {
    list.iter().any(|d| suffix_match(host, d))
}

fn host_blocked(host: &str) -> bool {
    let set = block_set();
    let mut h = host;
    loop {
        if set.contains(h) {
            return true;
        }
        match h.split_once('.') {
            Some((_, rest)) => h = rest,
            None => return false,
        }
    }
}

pub fn is_ad_url(uri: &str) -> bool {
    if !(uri.starts_with("http") || uri.starts_with("ws")) {
        return false;
    }
    let Some(host) = host_of(uri) else { return false };
    if host_blocked(&lower(host)) {
        return true;
    }
    let path = lower(path_of(uri));
    PATH_MARKERS.iter().any(|m| path.contains(m))
}

#[cfg(windows)]
mod imp {
    use super::{host_matches, is_ad_url};
    use std::sync::atomic::Ordering;
    use std::sync::{Arc, Mutex};
    use webview2_com::Microsoft::Web::WebView2::Win32::COREWEBVIEW2_WEB_RESOURCE_CONTEXT_ALL;
    use webview2_com::WebResourceRequestedEventHandler;
    use windows::core::w;
    use windows::Win32::System::Com::IStream;

    use crate::{ADBLOCK, ALLOW};

    fn is_allowed(current_host: &Arc<Mutex<String>>) -> bool {
        let Ok(host) = current_host.lock() else { return false };
        if host.is_empty() {
            return false;
        }
        let Ok(allow) = ALLOW.lock() else { return false };
        host_matches(&host, &allow)
    }

    fn is_blocked(uri: &str, current_host: &Arc<Mutex<String>>) -> bool {
        is_ad_url(uri) && !is_allowed(current_host)
    }

    pub fn install<R: tauri::Runtime>(webview: &tauri::Webview<R>, current_host: Arc<Mutex<String>>) {
        let _ = webview.with_webview(move |pw| unsafe {
            let controller = pw.controller();
            let Ok(core) = controller.CoreWebView2() else { return };
            let environment = pw.environment();

            let _ = core.AddWebResourceRequestedFilter(w!("*"), COREWEBVIEW2_WEB_RESOURCE_CONTEXT_ALL);

            let host_for_handler = current_host.clone();
            let env_for_handler = environment.clone();
            let handler = WebResourceRequestedEventHandler::create(Box::new(move |_sender, args| {
                let Some(args) = args else { return Ok(()) };
                if !ADBLOCK.load(Ordering::SeqCst) {
                    return Ok(());
                }
                let request = args.Request()?;
                let mut raw = windows::core::PWSTR::null();
                request.Uri(&mut raw)?;
                let uri = if raw.0.is_null() { String::new() } else { raw.to_string().unwrap_or_default() };
                if !raw.0.is_null() {
                    windows::Win32::System::Com::CoTaskMemFree(Some(raw.0 as *const _));
                }
                if is_blocked(&uri, &host_for_handler) {
                    if let Ok(response) = env_for_handler.CreateWebResourceResponse(
                        None::<&IStream>,
                        403,
                        w!("Blocked"),
                        w!(""),
                    ) {
                        let _ = args.SetResponse(Some(&response));
                    }
                }
                Ok(())
            }));
            let mut token: i64 = 0;
            let _ = core.add_WebResourceRequested(&handler, &mut token);
        });
    }
}

#[cfg(windows)]
pub use imp::install;

#[cfg(not(windows))]
pub fn install<R: tauri::Runtime>(_webview: &tauri::Webview<R>, _current_host: std::sync::Arc<std::sync::Mutex<String>>) {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blocks_exact_and_subdomains() {
        assert!(is_ad_url("https://doubleclick.net/x"));
        assert!(is_ad_url("https://stats.g.doubleclick.net/collect?v=1"));
        assert!(is_ad_url("https://A.B.Criteo.COM/z"));
        assert!(is_ad_url("wss://ib.adnxs.com/socket"));
    }

    #[test]
    fn keeps_lookalikes_and_clean_hosts() {
        assert!(!is_ad_url("https://notdoubleclick.net/x"));
        assert!(!is_ad_url("https://doubleclick.net.example.org/x"));
        assert!(!is_ad_url("https://fr.wikipedia.org/wiki/Publicite"));
        assert!(!is_ad_url("https://example.com/"));
    }

    #[test]
    fn path_markers() {
        assert!(is_ad_url("https://example.com/PAGEAD/x.js"));
        assert!(is_ad_url("https://example.com/foo/adserver/bar"));
        assert!(!is_ad_url("https://example.com/shadow/bar"));
    }

    #[test]
    fn ignores_other_schemes() {
        assert!(!is_ad_url("data:image/png;base64,AAAA/pagead/AAAA"));
        assert!(!is_ad_url("blob:https://example.com/1234"));
        assert!(!is_ad_url("tauri://localhost/index.html"));
    }

    #[test]
    fn userinfo_and_port() {
        assert!(is_ad_url("https://user:pw@ads.yahoo.com:8443/x"));
        assert_eq!(host_of("https://user@a.b:80/p?q"), Some("a.b"));
        assert_eq!(path_of("https://a.b/p?q"), "/p?q");
        assert_eq!(path_of("https://a.b"), "");
    }

    #[test]
    fn allow_list() {
        let list = vec!["example.com".to_string(), "x.org".to_string()];
        assert!(host_matches("example.com", &list));
        assert!(host_matches("www.example.com", &list));
        assert!(!host_matches("badexample.com", &list));
        assert!(!host_matches("com", &list));
    }
}
