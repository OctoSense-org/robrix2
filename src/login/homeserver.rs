//! Resolve sign-in destinations before sending any credentials.
use anyhow::{bail, Result};
use matrix_sdk::{
    config::RequestConfig,
    ruma::{
        api::client::{session::get_login_types::v3::LoginType, uiaa::UserIdentifier},
        UserId,
    },
    Client,
};
use url::Url;

/// An explicit server wins; otherwise discover from a full Matrix ID. A local
/// username uses matrix.org. Email domains are never treated as homeservers.
pub fn login_server(user: &str, server: Option<&str>) -> Result<String> {
    let user = user.trim();
    let server = server.map(str::trim).filter(|s| !s.is_empty());
    if let Some(server) = server {
        if server.chars().any(char::is_whitespace) || server.contains('\\') {
            bail!("Enter a server name or an HTTP/HTTPS homeserver URL.");
        }
        if server.contains("://") {
            let url = Url::parse(server)?;
            if !matches!(url.scheme(), "https" | "http")
                || url.host_str().is_none()
                || !url.username().is_empty()
                || url.password().is_some()
                || url.query().is_some()
                || url.fragment().is_some()
            {
                bail!("Use an HTTP/HTTPS homeserver URL without credentials, query, or fragment.");
            }
            return Ok(url.to_string());
        }
        let name = matrix_sdk::ruma::ServerName::parse(server)
            .map_err(|_| anyhow::anyhow!("Enter a valid server name, such as matrix.org."))?;
        return Ok(name.to_string());
    }
    if user.starts_with('@') && user.contains(':') {
        let id = UserId::parse(user)
            .map_err(|_| anyhow::anyhow!("Use a Matrix ID like @name:example.org."))?;
        return Ok(id.server_name().to_string());
    }
    if !user.starts_with('@') && user.contains('@') {
        bail!("Enter your homeserver when signing in with an email address.");
    }
    Ok("matrix.org".to_owned())
}

pub fn password_identifier(user: &str) -> Result<UserIdentifier> {
    let user = user.trim();
    if user.is_empty() || user.chars().any(char::is_whitespace) {
        bail!("Enter your Matrix ID, username, or registered email address.");
    }
    if !user.starts_with('@') && user.contains('@') {
        return Ok(UserIdentifier::Email(
            matrix_sdk::ruma::api::client::uiaa::EmailUserIdentifier::new(user.to_owned()),
        ));
    }
    if user.contains(':') {
        UserId::parse(user)
            .map_err(|_| anyhow::anyhow!("Use a Matrix ID like @name:example.org."))?;
    }
    Ok(UserIdentifier::Matrix(
        matrix_sdk::ruma::api::client::uiaa::MatrixUserIdentifier::new(
            if user.contains(':') {
                user
            } else {
                user.trim_start_matches('@')
            }
            .to_owned(),
        ),
    ))
}

#[derive(Clone, Debug)]
pub struct LoginMethods {
    pub homeserver: String,
    pub password: bool,
    pub sso: bool,
    pub providers: Vec<String>,
}

pub async fn login_methods(client: &Client) -> Result<LoginMethods> {
    let response = client.matrix_auth().get_login_types().await?;
    let mut methods = LoginMethods {
        homeserver: client.homeserver().to_string(),
        password: false,
        sso: false,
        providers: Vec::new(),
    };
    for flow in response.flows {
        match flow {
            LoginType::Password(_) => methods.password = true,
            LoginType::Sso(sso) => {
                methods.sso = true;
                methods
                    .providers
                    .extend(sso.identity_providers.into_iter().map(|p| p.name));
            }
            _ => {}
        }
    }
    Ok(methods)
}

/// Memory-only discovery: checking a server does not create a session/database.
pub async fn discover(user: &str, server: &str) -> Result<LoginMethods> {
    let destination = login_server(user, Some(server))?;
    let builder = Client::builder()
        .server_name_or_homeserver_url(destination)
        .request_config(
            RequestConfig::new()
                .timeout(std::time::Duration::from_secs(15))
                .retry_limit(0),
        );
    let client = crate::sliding_sync::use_android_tls_roots(builder)
        .build()
        .await?;
    login_methods(&client).await
}

#[cfg(not(target_os = "ios"))]
pub async fn browser_login<F, Fut>(client: &Client, open: F) -> matrix_sdk::Result<()>
where
    F: FnOnce(String) -> Fut + Send + 'static,
    Fut: std::future::Future<Output = matrix_sdk::Result<()>> + Send + 'static,
{
    client
        .matrix_auth()
        .login_sso(open)
        .initial_device_display_name("Robrix")
        .request_refresh_token()
        .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn full_id_discovers_its_server_and_explicit_url_wins() {
        assert_eq!(
            login_server(" @alex:example.org:8448 ", None).unwrap(),
            "example.org:8448"
        );
        assert_eq!(
            login_server("@alex:example.org", Some(" http://127.0.0.1:18120/ ")).unwrap(),
            "http://127.0.0.1:18120/"
        );
        assert_eq!(login_server("alex", None).unwrap(), "matrix.org");
        assert!(login_server("alex@example.org", None).is_err());
    }
    #[test]
    fn malformed_destinations_are_rejected_before_credentials() {
        for server in [
            "javascript://host",
            "https://user:secret@example.org",
            "https://example.org?token=secret",
            "https://example.org/#login",
            "foo bar",
            "https://example.org\\evil",
        ] {
            assert!(login_server("alex", Some(server)).is_err(), "{server}");
        }
        assert!(login_server("@alex:", None).is_err());
    }
    #[test]
    fn identifiers_use_matrix_protocol_types() {
        assert_eq!(
            serde_json::to_value(password_identifier("@alex").unwrap()).unwrap(),
            serde_json::json!({"type":"m.id.user","user":"alex"})
        );
        assert_eq!(
            serde_json::to_value(password_identifier(" alex@example.org ").unwrap()).unwrap(),
            serde_json::json!({"type":"m.id.thirdparty","medium":"email","address":"alex@example.org"})
        );
        assert!(password_identifier(" ").is_err());
        assert!(password_identifier("@alex:example.org").is_ok());
    }

    /// A local Matrix protocol fixture; no production credentials or browser.
    struct Server {
        url: String,
        stop: std::sync::Arc<std::sync::atomic::AtomicBool>,
        requests: std::sync::Arc<std::sync::Mutex<Vec<(String, serde_json::Value)>>>,
        thread: Option<std::thread::JoinHandle<()>>,
    }
    impl Server {
        fn new() -> Self {
            use std::{
                io::{Read, Write},
                sync::{
                    Arc, Mutex,
                    atomic::{AtomicBool, Ordering},
                },
            };
            let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            let url = format!("http://{}", listener.local_addr().unwrap());
            listener.set_nonblocking(true).unwrap();
            let stop = Arc::new(AtomicBool::new(false));
            let stopped = stop.clone();
            let requests = Arc::new(Mutex::new(Vec::new()));
            let received = requests.clone();
            let thread = std::thread::spawn(move || {
                while !stopped.load(Ordering::Relaxed) {
                    let Ok((mut socket, _)) = listener.accept() else {
                        std::thread::sleep(std::time::Duration::from_millis(5));
                        continue;
                    };
                    socket.set_nonblocking(false).unwrap();
                    socket
                        .set_read_timeout(Some(std::time::Duration::from_secs(10)))
                        .unwrap();
                    let mut bytes = Vec::new();
                    let mut buf = [0; 4096];
                    let (head, body) = loop {
                        let count = socket.read(&mut buf).unwrap();
                        if count == 0 {
                            panic!("Incomplete fixture request");
                        }
                        bytes.extend_from_slice(&buf[..count]);
                        if let Some(end) = bytes.windows(4).position(|w| w == b"\r\n\r\n") {
                            let head = String::from_utf8(bytes[..end].to_vec()).unwrap();
                            let len = head
                                .lines()
                                .find_map(|l| {
                                    l.to_ascii_lowercase()
                                        .strip_prefix("content-length:")
                                        .map(|n| n.trim().parse::<usize>().unwrap())
                                })
                                .unwrap_or(0);
                            if bytes.len() >= end + 4 + len {
                                break (head, bytes[end + 4..end + 4 + len].to_vec());
                            }
                        }
                    };
                    let route = head.lines().next().unwrap().to_owned();
                    let json = serde_json::from_slice(&body).unwrap_or(serde_json::Value::Null);
                    received.lock().unwrap().push((route.clone(), json));
                    let response = if route.contains("/versions ") {
                        serde_json::json!({"versions":["v1.11"],"unstable_features":{}})
                    } else if route.starts_with("GET ") && route.contains("/login ") {
                        serde_json::json!({"flows":[{"type":"m.login.password"},{"type":"m.login.sso","identity_providers":[{"id":"company-custom-id","name":"Company SSO"}]},{"type":"m.login.token"}]})
                    } else if route.starts_with("POST ") && route.contains("/login ") {
                        serde_json::json!({"user_id":"@fixture:localhost","device_id":"TESTDEVICE","access_token":"fixture-access","refresh_token":"fixture-refresh"})
                    } else {
                        serde_json::json!({})
                    };
                    let body = response.to_string();
                    let _ = write!(socket, "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}", body.len(), body);
                }
            });
            Self {
                url,
                stop,
                requests,
                thread: Some(thread),
            }
        }
        async fn client(&self) -> Client {
            Client::builder()
                .homeserver_url(&self.url)
                .request_config(RequestConfig::new().retry_limit(0))
                .build()
                .await
                .unwrap()
        }
    }
    impl Drop for Server {
        fn drop(&mut self) {
            self.stop.store(true, std::sync::atomic::Ordering::Relaxed);
            let result = self.thread.take().unwrap().join();
            if !std::thread::panicking() { result.unwrap(); }
        }
    }

    #[tokio::test]
    async fn reads_actual_advertised_methods_and_custom_provider_names() {
        let server = Server::new();
        let methods = login_methods(&server.client().await).await.unwrap();
        assert!(methods.password && methods.sso);
        assert_eq!(methods.providers, ["Company SSO"]);
    }

    #[cfg(not(target_os = "ios"))]
    #[tokio::test]
    async fn browser_callback_exchanges_token_and_requests_refresh() {
        let server = Server::new();
        let client = server.client().await;
        browser_login(&client, |link| async move {
            let url = Url::parse(&link).unwrap();
            assert!(url.path().ends_with("/login/sso/redirect"));
            let redirect = url
                .query_pairs()
                .find(|(k, _)| k == "redirectUrl")
                .unwrap()
                .1
                .into_owned();
            let mut callback = Url::parse(&redirect).unwrap();
            assert!(matches!(
                callback.host_str(),
                Some("127.0.0.1" | "[::1]" | "localhost")
            ));
            callback
                .query_pairs_mut()
                .append_pair("loginToken", "fixture-login-token");
            matrix_sdk::reqwest::get(callback)
                .await
                .unwrap()
                .error_for_status()
                .unwrap();
            Ok(())
        })
        .await
        .unwrap();
        assert_eq!(client.user_id().unwrap().as_str(), "@fixture:localhost");
        let requests = server.requests.lock().unwrap();
        let logins: Vec<_> = requests
            .iter()
            .filter(|(route, _)| route.starts_with("POST ") && route.contains("/login "))
            .collect();
        assert_eq!(logins.len(), 1);
        assert_eq!(logins[0].1["type"], "m.login.token");
        assert_eq!(logins[0].1["token"], "fixture-login-token");
        assert_eq!(logins[0].1["refresh_token"], true);
    }

    #[cfg(not(target_os = "ios"))]
    #[tokio::test]
    async fn cancellation_closes_callback_and_does_not_exchange_token() {
        let server = Server::new();
        let client = server.client().await;
        let (sender, receiver) = tokio::sync::oneshot::channel();
        let task = tokio::spawn(async move {
            browser_login(&client, |link| async move {
                let url = Url::parse(&link).unwrap();
                let redirect = url
                    .query_pairs()
                    .find(|(k, _)| k == "redirectUrl")
                    .unwrap()
                    .1
                    .into_owned();
                sender.send(redirect).unwrap();
                Ok(())
            })
            .await
        });
        let redirect = receiver.await.unwrap();
        task.abort();
        assert!(task.await.unwrap_err().is_cancelled());
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        assert!(matrix_sdk::reqwest::get(redirect).await.is_err());
        assert!(!server
            .requests
            .lock()
            .unwrap()
            .iter()
            .any(|(route, _)| route.starts_with("POST ") && route.contains("/login ")));
    }
}
