use super::*;
use std::process::Stdio;
use std::time::Duration;

struct TestClient {
    process: ProcessHandler,
    rpc: JsonRpcHandler,
    pending: PendingRequests,
    config: LspClientConfig,
    diagnostics: DiagnosticsStore,
}

#[async_trait]
impl LspClient for TestClient {
    fn get_process(&mut self) -> &mut ProcessHandler {
        &mut self.process
    }
    fn get_json_rpc(&mut self) -> &mut JsonRpcHandler {
        &mut self.rpc
    }
    fn get_pending_requests(&mut self) -> &mut PendingRequests {
        &mut self.pending
    }
    fn get_config(&self) -> &LspClientConfig {
        &self.config
    }
    fn get_diagnostics_store(&self) -> &DiagnosticsStore {
        &self.diagnostics
    }
    fn get_workspace_documents(&mut self) -> &mut WorkspaceDocumentsHandler {
        unreachable!("protocol test does not read workspace documents")
    }
}

const PROTOCOL: &str = r#"
import json, sys

def read():
    n = 0
    while True:
        line = sys.stdin.buffer.readline()
        if line == b'\r\n':
            break
        if line.lower().startswith(b'content-length:'):
            n = int(line.split(b':')[1])
    return json.loads(sys.stdin.buffer.read(n))

def send(message):
    data = json.dumps(dict(jsonrpc='2.0', **message)).encode()
    sys.stdout.buffer.write(('Content-Length: %d\r\n\r\n' % len(data)).encode() + data)
    sys.stdout.buffer.flush()

"#;

impl TestClient {
    async fn spawn(script: &str) -> Self {
        let script = format!("{PROTOCOL}\n{script}");
        let child = tokio::process::Command::new("python3")
            .args(["-u", "-c", &script])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .unwrap();
        let mut client = Self {
            process: ProcessHandler::new(child).await.unwrap(),
            rpc: JsonRpcHandler::new(),
            pending: PendingRequests::new(),
            config: LspClientConfig::default(),
            diagnostics: DiagnosticsStore::new(),
        };
        client.start_response_listener().await.unwrap();
        client
    }
}

#[tokio::test]
async fn server_progress_requests_do_not_consume_client_requests_with_the_same_id() {
    let script = r#"
request = read()
send(dict(id=request['id'], method='window/workDoneProgress/create', params=dict(token='workspace')))
ack = read()
assert ack['id'] == request['id'] and 'result' in ack
send(dict(method='$/progress', params=dict(token='workspace', value=dict(kind='begin', title='Loading'))))
send(dict(id=request['id'], result=dict(ok=True)))
request = read()
send(dict(method='$/progress', params=dict(token='workspace', value=dict(kind='end'))))
send(dict(id=request['id'], result=dict(ok=True)))
sys.stdin.buffer.read()
"#;
    let mut client = TestClient::spawn(script).await;
    assert_eq!(
        client.get_capabilities().window.unwrap().work_done_progress,
        Some(true)
    );
    assert_eq!(
        client
            .connection()
            .send_request("probe", None, client.config.request_timeout)
            .await
            .unwrap(),
        serde_json::json!({"ok": true})
    );
    assert!(client.pending.is_loading().await);
    client
        .connection()
        .send_request("finish", None, client.config.request_timeout)
        .await
        .unwrap();
    assert!(!client.pending.is_loading().await);
    client.process.kill().await.unwrap();
}

#[tokio::test]
async fn abandoned_requests_cancel_on_wire_and_release_pending_state() {
    let mut client = TestClient::spawn(
        r#"
held = None
status = None
cancelled = False
while True:
    request = read()
    if request.get("method") == "hold":
        held = request["id"]
        send(dict(method='probe/held', params={}))
    elif request.get("method") == "$/cancelRequest":
        assert request["params"]["id"] == held
        cancelled = True
        send(dict(id=held, result=None))
    else:
        status = request["id"]
    if cancelled and status is not None:
        send(dict(id=status, result=dict(cancelled=True)))
        break
sys.stdin.buffer.read()
"#,
    )
    .await;
    let mut held = client
        .pending
        .register_method_handler("probe/held".into())
        .await;
    {
        let request = client
            .connection()
            .send_request("hold", None, Duration::from_secs(30));
        tokio::pin!(request);
        tokio::select! {
            _ = &mut request => panic!("held request must not finish"),
            received = held.recv() => { assert!(received.is_some()); }
        }
    }
    let reply = client
        .connection()
        .send_request("status", None, Duration::from_secs(2))
        .await
        .unwrap();
    assert_eq!(reply, serde_json::json!({"cancelled": true}));
    assert!(client
        .pending
        .request_channels_for_test()
        .lock()
        .await
        .is_empty());
    client.process.kill().await.unwrap();
}

#[tokio::test]
async fn request_deadline_includes_waiting_for_a_connection_slot() {
    let mut client = TestClient::spawn(
        r#"
request = read()
send(dict(id=request["id"], result=dict(method=request["method"])))
sys.stdin.buffer.read()
"#,
    )
    .await;
    let mut permits = Vec::new();
    for _ in 0..crate::lsp::client::MAX_IN_FLIGHT_REQUESTS {
        permits.push(client.pending.acquire_slot().await);
    }
    let error = client
        .connection()
        .send_request("queued", None, Duration::from_millis(25))
        .await
        .unwrap_err();
    assert!(error.to_string().contains("timed out"));
    assert!(client
        .pending
        .request_channels_for_test()
        .lock()
        .await
        .is_empty());
    drop(permits);
    let reply = client
        .connection()
        .send_request("after", None, Duration::from_secs(2))
        .await
        .unwrap();
    assert_eq!(reply, serde_json::json!({"method": "after"}));
    client.process.kill().await.unwrap();
}
