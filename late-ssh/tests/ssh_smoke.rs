mod helpers;

use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use getrandom::SysRng;
use helpers::{new_test_db, test_app_state, test_config};
use late_ssh::ssh::run_with_listener;
use russh::keys::signature::rand_core::UnwrapErr;
use russh::{
    ChannelMsg, client,
    keys::{PrivateKey, PrivateKeyWithHashAlg},
};
use serde_json::{Value, json};
use std::sync::Arc;
use tokio::io::AsyncReadExt;
use tokio::net::{TcpListener, TcpStream};
use tokio::time::{Duration, timeout};

#[tokio::test]
async fn emits_ssh_banner_when_client_connects_over_tcp() {
    let test_db = new_test_db().await;
    let config = test_config(test_db.db.config().clone());
    let state = test_app_state(test_db.db.clone(), config);

    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let addr = listener.local_addr().expect("local addr");

    let handle = tokio::spawn(async move {
        let _ = run_with_listener(listener, state, None).await;
    });

    let connect = timeout(Duration::from_secs(2), TcpStream::connect(addr)).await;
    assert!(connect.is_ok(), "tcp connect timed out");
    let mut stream = connect.unwrap().expect("tcp connect failed");

    let mut banner = [0u8; 64];
    let n = timeout(Duration::from_secs(2), stream.read(&mut banner))
        .await
        .expect("banner read timeout")
        .expect("banner read");
    assert!(n > 0, "expected ssh banner bytes");
    assert!(
        std::str::from_utf8(&banner[..n])
            .unwrap_or("")
            .starts_with("SSH-2.0-"),
        "expected SSH identification banner"
    );

    handle.abort();
}

struct TestClient;

impl client::Handler for TestClient {
    type Error = russh::Error;

    async fn check_server_key(
        &mut self,
        _server_public_key: &russh::keys::ssh_key::PublicKey,
    ) -> Result<bool, Self::Error> {
        Ok(true)
    }
}

#[tokio::test]
async fn rejects_second_auth_when_ssh_attempt_rate_limit_is_one() {
    let test_db = new_test_db().await;
    let mut config = test_config(test_db.db.config().clone());
    config.max_conns_per_ip = 100;
    config.ssh_max_attempts_per_ip = 1;
    let state = test_app_state(test_db.db.clone(), config);

    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let addr = listener.local_addr().expect("local addr");
    let handle = tokio::spawn(async move {
        let _ = run_with_listener(listener, state, None).await;
    });

    let user = "rate-limit-user";
    let key = Arc::new(
        PrivateKey::random(
            &mut UnwrapErr(SysRng),
            russh::keys::ssh_key::Algorithm::Ed25519,
        )
        .expect("generate client key"),
    );

    let mut c1 = client::connect(Arc::new(client::Config::default()), addr, TestClient)
        .await
        .expect("connect client 1");
    let auth1 = c1
        .authenticate_publickey(
            user,
            PrivateKeyWithHashAlg::new(
                key.clone(),
                c1.best_supported_rsa_hash()
                    .await
                    .expect("rsa hash")
                    .flatten(),
            ),
        )
        .await
        .expect("auth client 1")
        .success();
    assert!(auth1, "first auth should succeed");
    c1.disconnect(russh::Disconnect::ByApplication, "", "en")
        .await
        .expect("disconnect client 1");

    let mut c2 = client::connect(Arc::new(client::Config::default()), addr, TestClient)
        .await
        .expect("connect client 2");
    let auth2 = c2
        .authenticate_publickey(
            user,
            PrivateKeyWithHashAlg::new(
                key.clone(),
                c2.best_supported_rsa_hash()
                    .await
                    .expect("rsa hash")
                    .flatten(),
            ),
        )
        .await
        .expect("auth client 2")
        .success();
    assert!(!auth2, "second auth should be rejected by ssh rate limiter");

    handle.abort();
}

#[tokio::test]
async fn closing_token_exec_channel_does_not_close_interactive_shell() {
    let test_db = new_test_db().await;
    let config = test_config(test_db.db.config().clone());
    let state = test_app_state(test_db.db.clone(), config);

    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let addr = listener.local_addr().expect("local addr");
    let handle = tokio::spawn(async move {
        let _ = run_with_listener(listener, state, None).await;
    });

    let user = "token-channel-user";
    let key = Arc::new(
        PrivateKey::random(
            &mut UnwrapErr(SysRng),
            russh::keys::ssh_key::Algorithm::Ed25519,
        )
        .expect("generate client key"),
    );
    let mut client = client::connect(Arc::new(client::Config::default()), addr, TestClient)
        .await
        .expect("connect client");
    let auth = client
        .authenticate_publickey(
            user,
            PrivateKeyWithHashAlg::new(
                key,
                client
                    .best_supported_rsa_hash()
                    .await
                    .expect("rsa hash")
                    .flatten(),
            ),
        )
        .await
        .expect("auth client")
        .success();
    assert!(auth, "auth should succeed");

    let mut token_channel = client
        .channel_open_session()
        .await
        .expect("open token channel");
    token_channel
        .exec(true, "late-cli-token-v1")
        .await
        .expect("exec token request");
    let mut token_payload = Vec::new();
    while token_payload.is_empty() {
        match timeout(Duration::from_secs(15), token_channel.wait())
            .await
            .expect("token response timeout")
            .expect("token channel closed before data")
        {
            ChannelMsg::Data { data } => token_payload.extend_from_slice(data.as_ref()),
            ChannelMsg::Close => panic!("token channel closed before data"),
            _ => {}
        }
    }
    assert!(
        std::str::from_utf8(&token_payload)
            .expect("token payload utf8")
            .contains("session_token"),
        "token exec should return session JSON"
    );

    let mut shell_channel = client
        .channel_open_session()
        .await
        .expect("open shell channel");
    shell_channel
        .request_pty(true, "xterm-256color", 80, 24, 0, 0, &[])
        .await
        .expect("request pty");
    shell_channel
        .request_shell(true)
        .await
        .expect("request shell");
    expect_shell_data(&mut shell_channel).await;
    drain_shell_data(&mut shell_channel).await;

    token_channel.close().await.expect("close token channel");
    shell_channel
        .data(&b" "[..])
        .await
        .expect("send shell input after token close");
    expect_shell_data(&mut shell_channel).await;

    client
        .disconnect(russh::Disconnect::ByApplication, "", "en")
        .await
        .expect("disconnect client");
    handle.abort();
}

#[tokio::test]
async fn whoami_exec_does_not_create_user_but_token_exec_still_does() {
    let test_db = new_test_db().await;
    let config = test_config(test_db.db.config().clone());
    let state = test_app_state(test_db.db.clone(), config);

    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let addr = listener.local_addr().expect("local addr");
    let handle = tokio::spawn(async move {
        let _ = run_with_listener(listener, state, None).await;
    });

    let initial_user_count = user_count(&test_db.db).await;
    let user = "whoami-user";
    let key = Arc::new(
        PrivateKey::random(
            &mut UnwrapErr(SysRng),
            russh::keys::ssh_key::Algorithm::Ed25519,
        )
        .expect("generate client key"),
    );
    let mut client = client::connect(Arc::new(client::Config::default()), addr, TestClient)
        .await
        .expect("connect client");
    let auth = client
        .authenticate_publickey(
            user,
            PrivateKeyWithHashAlg::new(
                key,
                client
                    .best_supported_rsa_hash()
                    .await
                    .expect("rsa hash")
                    .flatten(),
            ),
        )
        .await
        .expect("auth client")
        .success();
    assert!(auth, "auth should succeed");
    assert_eq!(
        user_count(&test_db.db).await,
        initial_user_count,
        "SSH auth alone must not create a user"
    );

    let (whoami_payload, whoami_status) = exec_request(&client, "late-cli-whoami-v1").await;
    assert_eq!(whoami_status, Some(0), "whoami should exit successfully");
    let whoami: Value = serde_json::from_slice(&whoami_payload).expect("whoami JSON");
    assert_eq!(whoami["status"], "nobody");
    assert!(whoami["ssh_fingerprint"].as_str().is_some());
    assert_eq!(
        user_count(&test_db.db).await,
        initial_user_count,
        "whoami must not create a user"
    );

    let (token_payload, token_status) = exec_request(&client, "late-cli-token-v1").await;
    assert_eq!(token_status, Some(0), "token should exit successfully");
    let token: Value = serde_json::from_slice(&token_payload).expect("token JSON");
    assert!(token["session_token"].as_str().is_some());
    assert_eq!(
        user_count(&test_db.db).await,
        initial_user_count + 1,
        "token exec should still materialize an account"
    );

    client
        .disconnect(russh::Disconnect::ByApplication, "", "en")
        .await
        .expect("disconnect client");
    handle.abort();
}

async fn user_count(db: &late_core::db::Db) -> i64 {
    let client = db.get().await.expect("db client");
    client
        .query_one("SELECT COUNT(*) FROM users", &[])
        .await
        .expect("count users")
        .get(0)
}

async fn exec_request(
    client: &client::Handle<TestClient>,
    command: &str,
) -> (Vec<u8>, Option<u32>) {
    let mut channel = client.channel_open_session().await.expect("open channel");
    channel.exec(true, command).await.expect("exec request");

    let mut payload = Vec::new();
    let mut exit_status = None;
    loop {
        match timeout(Duration::from_secs(15), channel.wait())
            .await
            .expect("exec response timeout")
        {
            Some(ChannelMsg::Data { data }) => payload.extend_from_slice(data.as_ref()),
            Some(ChannelMsg::ExitStatus { exit_status: code }) => exit_status = Some(code),
            Some(ChannelMsg::Close) | None => return (payload, exit_status),
            Some(_) => {}
        }
    }
}

async fn expect_shell_data(channel: &mut russh::Channel<client::Msg>) {
    loop {
        match timeout(Duration::from_secs(15), channel.wait()).await {
            Ok(Some(ChannelMsg::Data { .. })) => return,
            Ok(Some(ChannelMsg::Close)) => panic!("interactive shell closed unexpectedly"),
            Ok(Some(_)) => {}
            Ok(None) => panic!("interactive shell channel ended unexpectedly"),
            Err(_) => panic!("timed out waiting for interactive shell data"),
        }
    }
}

async fn drain_shell_data(channel: &mut russh::Channel<client::Msg>) {
    loop {
        match timeout(Duration::from_millis(100), channel.wait()).await {
            Ok(Some(ChannelMsg::Data { .. })) => {}
            Ok(Some(ChannelMsg::Close)) => panic!("interactive shell closed unexpectedly"),
            Ok(Some(_)) => {}
            Ok(None) => panic!("interactive shell channel ended unexpectedly"),
            Err(_) => return,
        }
    }
}

struct TestKey {
    key: Arc<PrivateKey>,
    openssh_public: String,
    fingerprint: String,
}

fn generate_key() -> TestKey {
    let key = PrivateKey::random(
        &mut UnwrapErr(SysRng),
        russh::keys::ssh_key::Algorithm::Ed25519,
    )
    .expect("generate key");
    let public = key.public_key();
    let openssh_public = public.to_openssh().expect("encode openssh public key");
    let fingerprint = public.fingerprint(russh::keys::HashAlg::Sha256).to_string();
    TestKey {
        key: Arc::new(key),
        openssh_public,
        fingerprint,
    }
}

async fn start_server() -> (
    late_core::test_utils::TestDb,
    std::net::SocketAddr,
    tokio::task::JoinHandle<()>,
) {
    let test_db = new_test_db().await;
    let config = test_config(test_db.db.config().clone());
    let state = test_app_state(test_db.db.clone(), config);
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let addr = listener.local_addr().expect("local addr");
    let handle = tokio::spawn(async move {
        let _ = run_with_listener(listener, state, None).await;
    });
    (test_db, addr, handle)
}

async fn connect_and_auth(
    addr: std::net::SocketAddr,
    login: &str,
    key: &Arc<PrivateKey>,
) -> client::Handle<TestClient> {
    let mut client = client::connect(Arc::new(client::Config::default()), addr, TestClient)
        .await
        .expect("connect client");
    let auth = client
        .authenticate_publickey(
            login,
            PrivateKeyWithHashAlg::new(
                key.clone(),
                client
                    .best_supported_rsa_hash()
                    .await
                    .expect("rsa hash")
                    .flatten(),
            ),
        )
        .await
        .expect("auth client")
        .success();
    assert!(auth, "auth should succeed");
    client
}

fn associate_key_command(public_key: &str) -> String {
    let payload = json!({ "public_key": public_key });
    let encoded = URL_SAFE_NO_PAD.encode(serde_json::to_vec(&payload).expect("encode payload"));
    format!("late-cli-associate-key-v1 {encoded}")
}

/// Materialize a late.sh account for the authenticated key via the token exec.
async fn materialize_account(client: &client::Handle<TestClient>) {
    let (payload, status) = exec_request(client, "late-cli-token-v1").await;
    assert_eq!(status, Some(0), "token exec should materialize an account");
    let token: Value = serde_json::from_slice(&payload).expect("token JSON");
    assert!(token["session_token"].as_str().is_some());
}

#[tokio::test]
async fn associate_key_attaches_new_fingerprint_to_account_and_is_idempotent() {
    let (_test_db, addr, handle) = start_server().await;

    // The user authenticates with their first key and materializes an account.
    let auth_key = generate_key();
    let client = connect_and_auth(addr, "assoc-user", &auth_key.key).await;
    materialize_account(&client).await;

    // Attach a second (dedicated) key to the same account.
    let dedicated = generate_key();
    let (payload, status) =
        exec_request(&client, &associate_key_command(&dedicated.openssh_public)).await;
    assert_eq!(status, Some(0), "associate-key should succeed");
    let resp: Value = serde_json::from_slice(&payload).expect("associate JSON");
    assert_eq!(resp["status"], "associated");
    assert_eq!(resp["ssh_fingerprint"], dedicated.fingerprint);
    let username = resp["username"].as_str().expect("username").to_string();

    // Idempotent: associating the same key again is still a success for the account.
    let (payload2, status2) =
        exec_request(&client, &associate_key_command(&dedicated.openssh_public)).await;
    assert_eq!(
        status2,
        Some(0),
        "re-associating the same key should succeed"
    );
    let resp2: Value = serde_json::from_slice(&payload2).expect("associate JSON");
    assert_eq!(resp2["status"], "associated");
    assert_eq!(resp2["username"], username);

    // The dedicated key now authenticates into the same account.
    let dedicated_client = connect_and_auth(addr, "ignored-login", &dedicated.key).await;
    let (whoami_payload, whoami_status) =
        exec_request(&dedicated_client, "late-cli-whoami-v1").await;
    assert_eq!(whoami_status, Some(0));
    let whoami: Value = serde_json::from_slice(&whoami_payload).expect("whoami JSON");
    assert_eq!(whoami["status"], "known");
    assert_eq!(whoami["username"], username);
    assert_eq!(whoami["ssh_fingerprint"], dedicated.fingerprint);

    client
        .disconnect(russh::Disconnect::ByApplication, "", "en")
        .await
        .expect("disconnect");
    dedicated_client
        .disconnect(russh::Disconnect::ByApplication, "", "en")
        .await
        .expect("disconnect");
    handle.abort();
}

#[tokio::test]
async fn associate_key_refuses_a_fingerprint_owned_by_another_account() {
    let (test_db, addr, handle) = start_server().await;

    // Two distinct accounts, each materialized under its own key.
    let key_a = generate_key();
    let client_a = connect_and_auth(addr, "owner-a", &key_a.key).await;
    materialize_account(&client_a).await;

    let key_b = generate_key();
    let client_b = connect_and_auth(addr, "owner-b", &key_b.key).await;
    materialize_account(&client_b).await;

    let users_before = user_count(&test_db.db).await;

    // Account A tries to claim B's key — must be refused, not silently stolen.
    let (payload, status) =
        exec_request(&client_a, &associate_key_command(&key_b.openssh_public)).await;
    assert_eq!(status, Some(1), "claiming another account's key must fail");
    let resp: Value = serde_json::from_slice(&payload).expect("error JSON");
    assert_eq!(resp["status"], "error");
    let message = resp["message"].as_str().unwrap_or_default();
    assert!(
        message.contains("already associated with another late.sh account"),
        "unexpected error message: {message}"
    );

    // Nothing created or removed; B's key still resolves to B.
    assert_eq!(user_count(&test_db.db).await, users_before);
    let (whoami_payload, whoami_status) = exec_request(&client_b, "late-cli-whoami-v1").await;
    assert_eq!(whoami_status, Some(0));
    let whoami: Value = serde_json::from_slice(&whoami_payload).expect("whoami JSON");
    assert_eq!(whoami["status"], "known");
    assert_eq!(whoami["ssh_fingerprint"], key_b.fingerprint);

    client_a
        .disconnect(russh::Disconnect::ByApplication, "", "en")
        .await
        .expect("disconnect");
    client_b
        .disconnect(russh::Disconnect::ByApplication, "", "en")
        .await
        .expect("disconnect");
    handle.abort();
}

#[tokio::test]
async fn interactive_pty_entry_materializes_account() {
    let (test_db, addr, handle) = start_server().await;
    let initial_user_count = user_count(&test_db.db).await;

    // No token exec: entering the interactive TUI alone must materialize the account.
    let key = generate_key();
    let client = connect_and_auth(addr, "pty-user", &key.key).await;

    let mut shell = client
        .channel_open_session()
        .await
        .expect("open shell channel");
    shell
        .request_pty(true, "xterm-256color", 80, 24, 0, 0, &[])
        .await
        .expect("request pty");
    shell.request_shell(true).await.expect("request shell");
    expect_shell_data(&mut shell).await;

    assert_eq!(
        user_count(&test_db.db).await,
        initial_user_count + 1,
        "interactive PTY entry should materialize a late.sh account"
    );

    client
        .disconnect(russh::Disconnect::ByApplication, "", "en")
        .await
        .expect("disconnect");
    handle.abort();
}
