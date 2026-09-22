use super::*;
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};
use tempfile::TempDir;
use wiremock::{
    matchers::{body_partial_json, header, method, path},
    Mock, MockServer, Request, ResponseTemplate,
};

#[derive(Clone, Default)]
struct MemoryVault(Arc<Mutex<HashMap<String, String>>>);

impl Vault for MemoryVault {
    /// Return test secrets without touching the real macOS keychain.
    fn get(&self, account: &str) -> Result<Option<String>> {
        Ok(self.0.lock().unwrap().get(account).cloned())
    }
    /// Store credentials inside this test only.
    fn set(&self, account: &str, secret: &str) -> Result<()> {
        self.0.lock().unwrap().insert(account.into(), secret.into());
        Ok(())
    }
    /// Emulate local disconnection.
    fn remove(&self, account: &str) -> Result<()> {
        self.0.lock().unwrap().remove(account);
        Ok(())
    }
}

struct Fixture {
    server: MockServer,
    dir: TempDir,
    connector: NewApi,
    tokens: Arc<Mutex<Vec<Value>>>,
}

/// Match rc.21's raw key contract and reject masked, malformed and double-prefixed values.
#[test]
fn full_key_accepts_raw_or_single_prefix_only() {
    let raw = "0123456789abcdefghijklmnopqrstuvwxyzABCDEFGHIJKL";
    assert_eq!(
        normalize_full_key(&json!(raw)).unwrap(),
        format!("sk-{raw}")
    );
    assert_eq!(
        normalize_full_key(&json!(format!("sk-{raw}"))).unwrap(),
        format!("sk-{raw}")
    );
    for invalid in [
        json!(null),
        json!(""),
        json!("sk-***masked***"),
        json!(format!("sk-sk-{raw}")),
        json!(format!("{raw}\n")),
    ] {
        assert_eq!(normalize_full_key(&invalid).unwrap_err().code, "key");
    }
}

/// Provide a fully local management/relay server; test code never creates real credentials.
async fn fixture() -> Fixture {
    let server = MockServer::start().await;
    let dir = tempfile::tempdir().unwrap();
    let mut connector = NewApi::new(dir.path().into(), Box::<MemoryVault>::default());
    let client = ApiClient::new(&server.uri()).unwrap();
    connector.session = Some(Session {
        client,
        user: user(),
        expires_at: now() + 86400,
        verified_at: now(),
    });
    Mock::given(method("GET")).and(path("/api/status")).respond_with(ResponseTemplate::new(200).set_body_json(json!({"success":true,"data":{
        "version":SUPPORTED_VERSION,"server_address":server.uri(),"custom_oauth_providers":[{"name":"Keycloak","slug":"keycloak","client_id":"public-client","authorization_endpoint":"https://idp.example/authorize","scopes":"openid profile email"}]
    }}))).mount(&server).await;
    Mock::given(method("GET"))
        .and(path("/api/user/self/groups"))
        .and(header("New-Api-User", "77"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({"success":true,"data":{"staff":{"desc":"员工组"}}})),
        )
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/user/models"))
        .and(header("New-Api-User", "77"))
        .respond_with(ResponseTemplate::new(200).set_body_json(
            json!({"success":true,"data":["model-one","responses-only","no-metadata"]}),
        ))
        .mount(&server)
        .await;
    Mock::given(method("GET")).and(path("/api/pricing")).respond_with(ResponseTemplate::new(200).set_body_json(json!({"success":true,"data":[
        {"model_name":"model-one","supported_endpoint_types":["openai","anthropic","unknown"]},
        {"model_name":"responses-only","supported_endpoint_types":["openai-response"]},
        {"model_name":"not-authorized","supported_endpoint_types":["openai"]}
    ]}))).mount(&server).await;
    Mock::given(method("GET"))
        .and(path("/v1/models"))
        .and(header(
            "Authorization",
            "Bearer sk-0123456789abcdefghijklmnopqrstuvwxyzABCDEFGHIJKL",
        ))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({"data":[{"id":"model-one"},{"id":"responses-only"}]})),
        )
        .mount(&server)
        .await;
    let tokens = Arc::new(Mutex::new(Vec::<Value>::new()));
    let search_tokens = tokens.clone();
    Mock::given(method("GET"))
        .and(path("/api/token/search"))
        .and(header("New-Api-User", "77"))
        .respond_with(move |request: &Request| {
            let keyword = request
                .url
                .query_pairs()
                .find(|(k, _)| k == "keyword")
                .unwrap()
                .1
                .into_owned();
            let items: Vec<_> = search_tokens
                .lock()
                .unwrap()
                .iter()
                .filter(|v| v["name"].as_str() == Some(&keyword))
                .cloned()
                .collect();
            ResponseTemplate::new(200)
                .set_body_json(json!({"success":true,"data":{"items":items,"total":items.len()}}))
        })
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/api/token/1/key"))
        .and(header("New-Api-User", "77"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({"success":true,"data":{"key":"0123456789abcdefghijklmnopqrstuvwxyzABCDEFGHIJKL"}})),
        )
        .mount(&server)
        .await;
    Fixture {
        server,
        dir,
        connector,
        tokens,
    }
}

/// Construct one stable fixture account with a non-default group.
fn user() -> User {
    User {
        id: 77,
        username: "fixture".into(),
        display_name: "Test user".into(),
        group: "staff".into(),
    }
}

/// Create a normal import request with the user-approved token defaults.
fn request(base: &str) -> ImportRequest {
    ImportRequest {
        base_url: base.into(),
        user_id: 77,
        group: "staff".into(),
        model_id: "model-one".into(),
        protocol: Protocol::OpenaiChat,
        name: "测试模型".into(),
        supports_tool_call: true,
        supports_images: false,
        context_window: None,
        reasoning_levels: vec![],
        replace_invalid: false,
        restart_uncertain: false,
    }
}

/// Emulate an insert that can succeed even when its HTTP response fails.
async fn mount_create(fixture: &Fixture, response_status: u16, expected: u64) {
    let tokens = fixture.tokens.clone();
    Mock::given(method("POST")).and(path("/api/token/")).and(header("New-Api-User", "77"))
        .and(body_partial_json(json!({"expired_time":-1,"unlimited_quota":true,"model_limits_enabled":true,"group":"staff"})))
        .respond_with(move |request: &Request| {
            let mut token: Value = request.body_json().unwrap();
            token["id"] = json!(1); token["user_id"] = json!(77); token["status"] = json!(1); token["key"] = json!("sk-***masked***");
            tokens.lock().unwrap().push(token);
            ResponseTemplate::new(response_status).set_body_json(json!({"success":true}))
        }).expect(expected).mount(&fixture.server).await;
}

/// URL and callback validation must reject credentials, non-HTTPS URLs, duplicate state and replay inputs.
#[test]
fn validates_urls_callbacks_and_native_base_conventions() {
    assert_eq!(
        instance_url(" https://new-api.example/ ").unwrap(),
        "https://new-api.example"
    );
    for raw in [
        "http://example.com",
        "https://u:p@example.com",
        "https://example.com/v1",
        "https://example.com?key=secret",
        "https://example.com/#secret",
    ] {
        assert!(instance_url(raw).is_err());
    }
    let expected = Url::parse("https://new-api.example/oauth/keycloak").unwrap();
    let good = Url::parse(
        "https://new-api.example/oauth/keycloak?state=random&code=secret&session_state=x",
    )
    .unwrap();
    assert_eq!(
        validate_callback(&expected, "random", &good).unwrap(),
        "secret"
    );
    for raw in [
        "https://evil.example/oauth/keycloak?state=random&code=secret",
        "https://new-api.example/oauth/other?state=random&code=secret",
        "https://new-api.example/oauth/keycloak?state=random&state=random&code=secret",
        "https://new-api.example/oauth/keycloak?state=wrong&code=secret",
        "https://new-api.example/oauth/keycloak?state=random&code=a&code=b",
        "https://new-api.example/oauth/keycloak?state=random&error=denied",
    ] {
        assert!(validate_callback(&expected, "random", &Url::parse(raw).unwrap()).is_err());
    }
    assert_eq!(
        api_base("https://site", Protocol::AnthropicMessages),
        "https://site"
    );
    assert_eq!(
        api_base("https://site", Protocol::OpenaiResponses),
        "https://site/v1"
    );
    assert!(token_name().len() <= 50);
}

/// The picker must exclude inaccessible models and models with no compatible protocol metadata.
#[tokio::test]
async fn catalog_intersects_permissions_and_protocols() {
    let mut f = fixture().await;
    let catalog = f
        .connector
        .catalog(&f.server.uri(), 77, None)
        .await
        .unwrap();
    assert_eq!(catalog.selected_group, "staff");
    assert_eq!(catalog.models.len(), 2);
    assert_eq!(
        catalog.models[0].protocols,
        vec![Protocol::OpenaiChat, Protocol::AnthropicMessages]
    );
    assert!(f
        .connector
        .catalog(&f.server.uri(), 77, Some("forbidden"))
        .await
        .is_err());
    assert_eq!(
        f.connector
            .catalog(&f.server.uri(), 88, None)
            .await
            .err()
            .unwrap()
            .code,
        "account_changed"
    );
}

/// Both a repeat import and another protocol must reuse one token and stable library identities.
#[tokio::test]
async fn creates_reads_full_key_and_reuses_across_protocols() {
    let mut f = fixture().await;
    mount_create(&f, 200, 1).await;
    let first = f
        .connector
        .prepare_import(request(&f.server.uri()))
        .await
        .unwrap();
    assert!(!first.reused);
    assert_eq!(
        first.model.api_key,
        "sk-0123456789abcdefghijklmnopqrstuvwxyzABCDEFGHIJKL"
    );
    let again = f
        .connector
        .prepare_import(request(&f.server.uri()))
        .await
        .unwrap();
    assert!(again.reused);
    assert_eq!(first.model.id, again.model.id);
    let mut anthropic = request(&f.server.uri());
    anthropic.protocol = Protocol::AnthropicMessages;
    let other = f.connector.prepare_import(anthropic).await.unwrap();
    assert!(other.reused);
    assert_eq!(other.model.base_url, f.server.uri());
    assert_ne!(other.model.id, first.model.id);
    assert_eq!(other.siblings.len(), 2);
    let journal = std::fs::read_to_string(f.dir.path().join("new-api.json")).unwrap();
    assert!(!journal.contains("sk-test-key"));
    assert!(!journal.contains("session="));
    let requests = f.server.received_requests().await.unwrap();
    assert!(requests.iter().all(|r| r.url.path() != "/api/user/token"));
    assert!(requests
        .iter()
        .filter(|r| r.url.path() == "/v1/models")
        .all(|r| !r.headers.contains_key("cookie") && !r.headers.contains_key("new-api-user")));
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            std::fs::metadata(f.dir.path().join("new-api.json"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
    }
}

/// A server-side insert followed by an HTTP failure must be recovered without a second POST.
#[tokio::test]
async fn recovers_insert_when_create_response_is_lost() {
    let mut f = fixture().await;
    mount_create(&f, 500, 1).await;
    let saved = f
        .connector
        .prepare_import(request(&f.server.uri()))
        .await
        .unwrap();
    assert_eq!(
        saved.model.api_key,
        "sk-0123456789abcdefghijklmnopqrstuvwxyzABCDEFGHIJKL"
    );
    f.connector
        .prepare_import(request(&f.server.uri()))
        .await
        .unwrap();
}

/// An unknown insert result remains recoverable across restarts and never blindly creates again.
#[tokio::test]
async fn uncertainty_survives_restart_without_duplicate_creation() {
    let mut f = fixture().await;
    Mock::given(method("POST"))
        .and(path("/api/token/"))
        .respond_with(ResponseTemplate::new(500))
        .expect(1)
        .mount(&f.server)
        .await;
    assert_eq!(
        f.connector
            .prepare_import(request(&f.server.uri()))
            .await
            .err()
            .unwrap()
            .code,
        "creation_uncertain"
    );
    let session = f.connector.session.take();
    let mut restarted = NewApi::new(f.dir.path().into(), Box::<MemoryVault>::default());
    restarted.session = session;
    assert_eq!(
        restarted
            .prepare_import(request(&f.server.uri()))
            .await
            .err()
            .unwrap()
            .code,
        "creation_uncertain"
    );
}

/// A later search must recover an uncertain insertion using its durable exact name.
#[tokio::test]
async fn resumes_pending_creation_after_server_becomes_visible() {
    let mut f = fixture().await;
    Mock::given(method("POST"))
        .and(path("/api/token/"))
        .respond_with(ResponseTemplate::new(500))
        .expect(1)
        .mount(&f.server)
        .await;
    let _ = f.connector.prepare_import(request(&f.server.uri())).await;
    let journal = f.connector.journal().unwrap();
    let r = &journal.records[0];
    f.tokens.lock().unwrap().push(json!({"id":1,"user_id":77,"name":r.name,"status":1,"group":"staff","model_limits_enabled":true,"model_limits":"model-one","expired_time":-1,"unlimited_quota":true}));
    let result = f
        .connector
        .prepare_import(request(&f.server.uri()))
        .await
        .unwrap();
    assert!(result.reused);
}

/// Changed remote restrictions require explicit replacement; callers cannot silently broaden access.
#[tokio::test]
async fn rejects_modified_or_disabled_tokens_and_ambiguous_names() {
    let mut f = fixture().await;
    mount_create(&f, 200, 1).await;
    f.connector
        .prepare_import(request(&f.server.uri()))
        .await
        .unwrap();
    f.tokens.lock().unwrap()[0]["model_limits"] = json!("model-one,another-model");
    assert_eq!(
        f.connector
            .prepare_import(request(&f.server.uri()))
            .await
            .err()
            .unwrap()
            .code,
        "token_invalid"
    );
    f.tokens.lock().unwrap()[0]["model_limits"] = json!("model-one");
    f.tokens.lock().unwrap()[0]["status"] = json!(2);
    assert_eq!(
        f.connector
            .prepare_import(request(&f.server.uri()))
            .await
            .err()
            .unwrap()
            .code,
        "token_invalid"
    );
    let duplicate = f.tokens.lock().unwrap()[0].clone();
    f.tokens.lock().unwrap().push(duplicate);
    assert_eq!(
        f.connector
            .prepare_import(request(&f.server.uri()))
            .await
            .err()
            .unwrap()
            .code,
        "ambiguous_token"
    );
}

/// Codex capabilities and protocol mismatches are rejected before remote token creation.
#[tokio::test]
async fn rejects_invalid_model_config_before_creating() {
    let mut f = fixture().await;
    let mut req = request(&f.server.uri());
    req.protocol = Protocol::OpenaiResponses;
    assert_eq!(
        f.connector.prepare_import(req).await.err().unwrap().code,
        "model"
    );
    let mut req = request(&f.server.uri());
    req.protocol = Protocol::OpenaiResponses;
    req.context_window = Some(128000);
    assert_eq!(
        f.connector.prepare_import(req).await.err().unwrap().code,
        "model"
    );
    assert!(f
        .server
        .received_requests()
        .await
        .unwrap()
        .iter()
        .all(|r| r.url.path() != "/api/token/"));
}

/// New API business errors must not be mistaken for success or reflected with their secret payloads.
#[tokio::test]
async fn checks_business_success_and_redacts_error_bodies() {
    let server = MockServer::start().await;
    Mock::given(path("/failure"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({"success":false,"message":"secret-token-in-response"})),
        )
        .mount(&server)
        .await;
    let api = ApiClient::new(&server.uri()).unwrap();
    let error = api
        .api(api.management(Method::GET, "/failure", None))
        .await
        .err()
        .unwrap();
    assert_eq!(error.code, "api_rejected");
    assert!(!error.message.contains("secret-token"));
    Mock::given(path("/redirect"))
        .respond_with(
            ResponseTemplate::new(302)
                .insert_header("location", format!("{}/failure", server.uri())),
        )
        .mount(&server)
        .await;
    assert_eq!(
        api.api(api.management(Method::GET, "/redirect", None))
            .await
            .err()
            .unwrap()
            .code,
        "redirect"
    );
}

/// Cancellation and expiry consume native flows without ever calling the OAuth exchange endpoint.
#[tokio::test]
async fn canceled_timed_out_and_replayed_logins_cannot_complete() {
    let mut f = fixture().await;
    Mock::given(path("/api/oauth/state"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(json!({"success":true,"data":"state-nonce"})),
        )
        .mount(&f.server)
        .await;
    let flow = f.connector.start_login(&f.server.uri()).await.unwrap();
    assert_eq!(
        flow.url
            .query_pairs()
            .find(|(k, _)| k == "redirect_uri")
            .unwrap()
            .1,
        format!("{}/oauth/keycloak", f.server.uri())
    );
    f.connector.cancel_login(&flow.id);
    let mut callback = flow.callback;
    callback.set_query(Some("state=state-nonce&code=one-time-code"));
    assert!(f.connector.finish_login(&flow.id, &callback).await.is_err());
    let next = f.connector.start_login(&f.server.uri()).await.unwrap();
    f.connector.pending.as_mut().unwrap().started = now() - LOGIN_TIMEOUT;
    assert_eq!(
        f.connector
            .finish_login(&next.id, &callback)
            .await
            .err()
            .unwrap()
            .code,
        "oauth_timeout"
    );
    assert!(f.connector.finish_login(&next.id, &callback).await.is_err());
    assert!(f
        .server
        .received_requests()
        .await
        .unwrap()
        .iter()
        .all(|r| r.url.path() != "/api/oauth/keycloak"));
}

/// Verify a successful callback stores a session in the vault and rejects replay.
#[tokio::test]
async fn oauth_verifies_identity_and_saves_only_in_vault() {
    let mut f = fixture().await;
    let vault = MemoryVault::default();
    f.connector.vault = Box::new(vault.clone());
    Mock::given(path("/api/oauth/state"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("set-cookie", "session=preauth; Path=/; HttpOnly")
                .set_body_json(json!({"success":true,"data":"state-nonce"})),
        )
        .mount(&f.server)
        .await;
    Mock::given(path("/api/oauth/keycloak"))
        .and(header("cookie", "session=preauth"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("set-cookie", "session=authenticated; Path=/; HttpOnly")
                .set_body_json(json!({"success":true,"data":user()})),
        )
        .expect(1)
        .mount(&f.server)
        .await;
    Mock::given(path("/api/user/self"))
        .and(header("cookie", "session=authenticated"))
        .and(header("New-Api-User", "77"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(json!({"success":true,"data":user()})),
        )
        .mount(&f.server)
        .await;
    let flow = f.connector.start_login(&f.server.uri()).await.unwrap();
    let mut callback = flow.callback;
    callback.set_query(Some("state=state-nonce&code=one-time-code"));
    f.connector.finish_login(&flow.id, &callback).await.unwrap();
    assert!(vault
        .get(&f.server.uri())
        .unwrap()
        .unwrap()
        .contains("session=authenticated"));
    assert!(!f.dir.path().join("new-api.json").exists());
    assert!(f.connector.finish_login(&flow.id, &callback).await.is_err());
    f.connector.disconnect(&f.server.uri()).unwrap();
    assert!(vault.get(&f.server.uri()).unwrap().is_none());
}

/// Closing the native window while exchange is running must prevent any credential persistence.
#[tokio::test]
async fn cancellation_during_exchange_does_not_save_session() {
    let mut f = fixture().await;
    let vault = MemoryVault::default();
    f.connector.vault = Box::new(vault.clone());
    f.connector.session = None;
    Mock::given(path("/api/oauth/state"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(json!({"success":true,"data":"state-nonce"})),
        )
        .mount(&f.server)
        .await;
    let flow = f.connector.start_login(&f.server.uri()).await.unwrap();
    let cancel = flow.canceled.clone();
    Mock::given(path("/api/oauth/keycloak"))
        .respond_with(move |_: &Request| {
            cancel.store(true, Ordering::SeqCst);
            ResponseTemplate::new(200)
                .insert_header("set-cookie", "session=authenticated; Path=/; HttpOnly")
                .set_body_json(json!({"success":true,"data":user()}))
        })
        .expect(1)
        .mount(&f.server)
        .await;
    Mock::given(path("/api/user/self"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(json!({"success":true,"data":user()})),
        )
        .mount(&f.server)
        .await;
    let mut callback = flow.callback;
    callback.set_query(Some("state=state-nonce&code=one-time-code"));
    assert_eq!(
        f.connector
            .finish_login(&flow.id, &callback)
            .await
            .unwrap_err()
            .code,
        "oauth_canceled"
    );
    assert!(f.connector.session.is_none());
    assert!(vault.get(&f.server.uri()).unwrap().is_none());
}

/// Expired or remotely revoked sessions cannot be used to create a key in a stale UI.
#[tokio::test]
async fn expired_sessions_require_login_and_forget_revoked_credentials() {
    let mut f = fixture().await;
    f.connector.session.as_mut().unwrap().expires_at = now() - 1;
    assert_eq!(
        f.connector.status(&f.server.uri()).await.unwrap().phase,
        "disconnected"
    );
    let mut f = fixture().await;
    let vault = MemoryVault::default();
    vault.set(&f.server.uri(), "saved-secret").unwrap();
    f.connector.vault = Box::new(vault.clone());
    f.connector.session.as_mut().unwrap().verified_at = 0;
    Mock::given(path("/api/user/self"))
        .respond_with(ResponseTemplate::new(401))
        .mount(&f.server)
        .await;
    assert_eq!(
        f.connector.status(&f.server.uri()).await.unwrap().phase,
        "disconnected"
    );
    assert!(vault.get(&f.server.uri()).unwrap().is_none());
    assert!(f.connector.session.is_none());
}

/// A corrupt journal must stop before any remote insert, preserving recoverability.
#[tokio::test]
async fn corrupt_journal_prevents_duplicate_creation() {
    let mut f = fixture().await;
    std::fs::write(f.dir.path().join("new-api.json"), "broken-json").unwrap();
    assert_eq!(
        f.connector
            .prepare_import(request(&f.server.uri()))
            .await
            .err()
            .unwrap()
            .code,
        "storage"
    );
    assert!(f
        .server
        .received_requests()
        .await
        .unwrap()
        .iter()
        .all(|r| r.url.path() != "/api/token/"));
}

/// A minimum inference is made only when explicitly called and uses the selected native protocol.
#[tokio::test]
async fn explicit_test_uses_correct_protocol_and_never_panel_credentials() {
    let mut f = fixture().await;
    mount_create(&f, 200, 1).await;
    let imported = f
        .connector
        .prepare_import(request(&f.server.uri()))
        .await
        .unwrap();
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .and(header(
            "authorization",
            "Bearer sk-0123456789abcdefghijklmnopqrstuvwxyzABCDEFGHIJKL",
        ))
        .and(body_partial_json(
            json!({"model":"model-one","stream":false,"max_tokens":64}),
        ))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({"choices":[{"message":{"content":"OK"}}]})),
        )
        .expect(1)
        .mount(&f.server)
        .await;
    assert!(f
        .connector
        .test_model(&imported.model)
        .await
        .unwrap()
        .contains("调用已验证"));
    let mut modified = imported.model;
    modified.base_url = "https://other.example/v1".into();
    assert_eq!(
        f.connector.test_model(&modified).await.err().unwrap().code,
        "model"
    );
}
