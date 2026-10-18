use hyper_util::client::proxy::matcher::Matcher;
use url::{Host, Url};

/// An HTTP proxy the relay socket tunnels through with `CONNECT`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HttpProxy {
    /// `host:port` to dial.
    pub(crate) authority: String,
    /// A ready `Proxy-Authorization` value, from credentials in the proxy URL.
    pub(crate) authorization: Option<String>,
}

impl HttpProxy {
    /// Parses a proxy as an env var or `ProxyServer` writes it: a scheme is optional and means
    /// `http`. Only plain HTTP proxies are supported; `https://` and `socks*` give None.
    pub fn parse(proxy: &str) -> Option<Self> {
        intercept(proxy, "", "relay")
    }
}

/// The `HKCU\…\Internet Settings` values that pick a manual proxy.
#[derive(Debug, Default, Clone)]
pub(crate) struct InternetSettings {
    pub enabled: bool,
    pub server: String,
    pub bypass: String,
}

/// The proxy the Windows settings and proxy env vars choose for `url`, as reqwest resolves them.
// TODO: PAC (AutoConfigURL) and WPAD auto-detect are not read; ClientWebSocket honoured both.
pub(crate) fn system_proxy(url: &Url) -> Option<HttpProxy> {
    resolve(url, |name| std::env::var(name).ok(), read_internet_settings)
}

/// Loopback is always direct, as WinINet does. Env vars win over the registry: the scheme's own
/// var, then `ALL_PROXY`, gated by `NO_PROXY`. The registry proxy is gated by `ProxyOverride` and
/// `NO_PROXY` both.
pub(crate) fn resolve(
    url: &Url,
    env: impl Fn(&str) -> Option<String>,
    registry: impl FnOnce() -> Option<InternetSettings>,
) -> Option<HttpProxy> {
    if is_loopback(&url.host()?) {
        return None;
    }
    let host = url.host_str()?;
    let secure = matches!(url.scheme(), "wss" | "https");
    let first = |names: &[&str]| names.iter().find_map(|name| env(name).filter(|v| !v.trim().is_empty()));
    let no_proxy = first(&["NO_PROXY", "no_proxy"]).unwrap_or_default();

    let scheme_vars: &[&str] = if secure { &["HTTPS_PROXY", "https_proxy"] } else { &["HTTP_PROXY", "http_proxy"] };
    if let Some(proxy) = first(scheme_vars).or_else(|| first(&["ALL_PROXY", "all_proxy"])) {
        return intercept(&proxy, &no_proxy, host);
    }

    let settings = registry().filter(|s| s.enabled)?;
    let proxy = server_for_scheme(&settings.server, if secure { "https" } else { "http" })?;
    if bypassed(&settings.bypass, url.host()?) {
        return None;
    }
    intercept(proxy, &no_proxy, host)
}

/// The proxy for `scheme` from `ProxyServer`: either one `host:port` for every scheme, or a
/// `http=host:port;https=host:port` list where a missing scheme means no proxy.
pub(crate) fn server_for_scheme<'a>(server: &'a str, scheme: &str) -> Option<&'a str> {
    let server = server.trim();
    if !server.contains('=') {
        return Some(server).filter(|s| !s.is_empty());
    }
    server
        .split(';')
        .filter_map(|entry| entry.split_once('='))
        .find(|(name, _)| name.trim().eq_ignore_ascii_case(scheme))
        .map(|(_, proxy)| proxy.trim())
        .filter(|proxy| !proxy.is_empty())
}

/// Whether `ProxyOverride` sends `host` direct: `;`-separated `*` globs, plus `<local>` for dotless
/// names and loopback (as .NET's HttpWindowsProxy treats it).
pub(crate) fn bypassed(overrides: &str, host: Host<&str>) -> bool {
    let name = match host {
        Host::Domain(domain) => domain.to_owned(),
        Host::Ipv4(ip) => ip.to_string(),
        Host::Ipv6(ip) => ip.to_string(),
    };
    overrides.split(';').map(str::trim).filter(|entry| !entry.is_empty()).any(|entry| {
        if entry.eq_ignore_ascii_case("<local>") {
            is_local(&host)
        } else {
            glob(&entry.to_ascii_lowercase(), &name.to_ascii_lowercase())
        }
    })
}

fn is_local(host: &Host<&str>) -> bool {
    is_loopback(host) || matches!(host, Host::Domain(domain) if !domain.contains('.'))
}

fn is_loopback(host: &Host<&str>) -> bool {
    match host {
        Host::Domain(domain) => domain.eq_ignore_ascii_case("localhost"),
        Host::Ipv4(ip) => ip.is_loopback(),
        Host::Ipv6(ip) => ip.is_loopback(),
    }
}

/// `*` matches any run of characters, including none; everything else matches itself.
fn glob(pattern: &str, text: &str) -> bool {
    let mut parts = pattern.split('*');
    let Some(mut rest) = text.strip_prefix(parts.next().unwrap_or_default()) else { return false };
    let parts: Vec<&str> = parts.collect();
    let Some((last, middle)) = parts.split_last() else { return rest.is_empty() };
    for part in middle {
        let Some(at) = rest.find(part) else { return false };
        rest = &rest[at + part.len()..];
    }
    rest.ends_with(last)
}

/// Parses `proxy` and applies `no_proxy` to `host` with hyper-util's matcher, the one reqwest uses.
fn intercept(proxy: &str, no_proxy: &str, host: &str) -> Option<HttpProxy> {
    let target: http::Uri = format!("http://{host}/").parse().ok()?;
    let intercept = Matcher::builder().all(proxy.trim()).no(no_proxy).build().intercept(&target)?;
    let uri = intercept.uri();
    if uri.scheme_str() != Some("http") {
        tracing::warn!(proxy = %uri, "Only HTTP proxies can tunnel the relay; connecting direct");
        return None;
    }
    let authority = uri.authority()?;
    Some(HttpProxy {
        authority: format!("{}:{}", authority.host(), authority.port_u16().unwrap_or(80)),
        authorization: intercept.basic_auth().and_then(|v| v.to_str().ok()).map(str::to_owned),
    })
}

#[cfg(windows)]
fn read_internet_settings() -> Option<InternetSettings> {
    let key = windows_registry::CURRENT_USER
        .open(r"Software\Microsoft\Windows\CurrentVersion\Internet Settings")
        .ok()?;
    Some(InternetSettings {
        enabled: key.get_u32("ProxyEnable").unwrap_or(0) != 0,
        server: key.get_string("ProxyServer").unwrap_or_default(),
        bypass: key.get_string("ProxyOverride").unwrap_or_default(),
    })
}

#[cfg(not(windows))]
fn read_internet_settings() -> Option<InternetSettings> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env(vars: &'static [(&str, &str)]) -> impl Fn(&str) -> Option<String> {
        move |name| vars.iter().find(|(k, _)| *k == name).map(|(_, v)| (*v).to_owned())
    }

    fn registry(server: &str, bypass: &str) -> impl FnOnce() -> Option<InternetSettings> {
        let settings = InternetSettings { enabled: true, server: server.into(), bypass: bypass.into() };
        move || Some(settings)
    }

    fn proxied(url: &str, vars: &'static [(&str, &str)], server: &str, bypass: &str) -> Option<String> {
        resolve(&Url::parse(url).unwrap(), env(vars), registry(server, bypass)).map(|p| p.authority)
    }

    const RELAY: &str = "wss://relay-eu.rustoverlay.com/v1/room";

    #[test]
    fn the_schemes_own_var_beats_all_proxy_which_beats_the_registry() {
        let both = &[("HTTPS_PROXY", "a:1"), ("ALL_PROXY", "b:2")];
        assert_eq!(proxied(RELAY, both, "c:3", "").as_deref(), Some("a:1"));
        assert_eq!(proxied(RELAY, &[("all_proxy", "b:2")], "c:3", "").as_deref(), Some("b:2"));
        assert_eq!(proxied(RELAY, &[], "c:3", "").as_deref(), Some("c:3"));
    }

    #[test]
    fn ws_reads_http_proxy_and_wss_reads_https_proxy() {
        let vars = &[("https_proxy", "secure:1"), ("http_proxy", "plain:2")];
        assert_eq!(proxied(RELAY, vars, "", "").as_deref(), Some("secure:1"));
        assert_eq!(proxied("ws://relay.example.com/", vars, "", "").as_deref(), Some("plain:2"));
        assert_eq!(proxied("ws://relay.example.com/", &[("HTTPS_PROXY", "secure:1")], "reg:3", "").as_deref(), Some("reg:3"));
    }

    #[test]
    fn no_proxy_sends_matching_hosts_direct_for_env_and_registry_proxies() {
        let vars = &[("HTTPS_PROXY", "a:1"), ("NO_PROXY", "rustoverlay.com, 10.0.0.0/8")];
        assert_eq!(proxied(RELAY, vars, "", ""), None);
        assert_eq!(proxied("wss://10.1.2.3/", vars, "", ""), None);
        assert_eq!(proxied("wss://relay.other.com/", vars, "", "").as_deref(), Some("a:1"));
        assert_eq!(proxied(RELAY, &[("no_proxy", "rustoverlay.com")], "c:3", ""), None);
    }

    #[test]
    fn credentials_in_the_proxy_url_become_basic_authorization() {
        let url = Url::parse(RELAY).unwrap();
        let proxy = resolve(&url, env(&[("HTTPS_PROXY", "http://user:p%40ss@proxy:3128")]), || None).unwrap();
        assert_eq!(proxy.authority, "proxy:3128");
        assert_eq!(proxy.authorization.as_deref(), Some("Basic dXNlcjpwQHNz"));
    }

    #[test]
    fn loopback_is_always_direct() {
        let vars = &[("ALL_PROXY", "a:1")];
        assert_eq!(proxied("ws://127.0.0.1:9000/", vars, "c:3", ""), None);
        assert_eq!(proxied("wss://localhost/", &[], "c:3", ""), None);
    }

    #[test]
    fn a_disabled_or_missing_registry_proxy_is_direct() {
        let url = Url::parse(RELAY).unwrap();
        let disabled = InternetSettings { enabled: false, server: "c:3".into(), bypass: String::new() };
        assert_eq!(resolve(&url, env(&[]), || Some(disabled)), None);
        assert_eq!(resolve(&url, env(&[]), || None), None);
    }

    #[test]
    fn proxy_server_is_one_proxy_for_all_or_a_per_scheme_list() {
        assert_eq!(server_for_scheme(" proxy:8080 ", "https"), Some("proxy:8080"));
        assert_eq!(server_for_scheme("proxy:8080", "http"), Some("proxy:8080"));
        assert_eq!(server_for_scheme("http=h:1;https=s:2;ftp=f:3", "https"), Some("s:2"));
        assert_eq!(server_for_scheme("HTTP=h:1; HTTPS = s:2", "http"), Some("h:1"));
        assert_eq!(server_for_scheme("http=h:1;socks=k:4", "https"), None);
        assert_eq!(server_for_scheme("", "https"), None);
    }

    fn skips(overrides: &str, host: &str) -> bool {
        bypassed(overrides, Url::parse(&format!("wss://{host}/")).unwrap().host().unwrap())
    }

    #[test]
    fn local_bypasses_dotless_names_and_loopback_only() {
        assert!(skips("<local>", "relaybox"));
        assert!(skips("<LOCAL>", "localhost"));
        assert!(skips("<local>", "127.0.0.1"));
        assert!(skips("<local>", "[::1]"));
        assert!(!skips("<local>", "relay-eu.rustoverlay.com"));
        assert!(!skips("<local>", "10.0.0.1"));
    }

    #[test]
    fn override_entries_are_case_insensitive_globs() {
        let list = "*.corp.lan; 10.*;RELAY-EU.RustOverlay.com;*overlay*";
        assert!(skips(list, "box.corp.lan"));
        assert!(!skips(list, "corp.lan"));
        assert!(skips(list, "10.20.30.40"));
        assert!(!skips(list, "110.20.30.40"));
        assert!(skips(list, "relay-eu.rustoverlay.com"));
        assert!(skips(list, "myoverlay.net"));
        assert!(!skips(list, "relay.example.com"));
    }

    #[test]
    fn the_registry_proxy_honours_its_per_scheme_list_and_overrides() {
        let (server, bypass) = ("http=h:1;https=s:2", "<local>;*.lan");
        assert_eq!(proxied(RELAY, &[], server, bypass).as_deref(), Some("s:2"));
        assert_eq!(proxied("ws://relay.example.com/", &[], server, bypass).as_deref(), Some("h:1"));
        assert_eq!(proxied("wss://box.lan/", &[], server, bypass), None);
        assert_eq!(proxied("ws://relaybox/", &[], server, bypass), None);
    }

    #[test]
    fn only_plain_http_proxies_parse() {
        assert_eq!(HttpProxy::parse("http://p:3128").map(|p| p.authority).as_deref(), Some("p:3128"));
        assert_eq!(HttpProxy::parse("p").map(|p| p.authority).as_deref(), Some("p:80"));
        assert_eq!(HttpProxy::parse("socks5://p:1080"), None);
        assert_eq!(HttpProxy::parse("https://p:443"), None);
    }
}
