use super::*;
use std::process::Stdio;

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

#[tokio::test]
async fn server_progress_requests_do_not_consume_client_requests_with_the_same_id() {
    let script = r#"
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
    let child = tokio::process::Command::new("python3")
        .args(["-u", "-c", script])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .unwrap();
    let mut client = TestClient {
        process: ProcessHandler::new(child).await.unwrap(),
        rpc: JsonRpcHandler::new(),
        pending: PendingRequests::new(),
        config: LspClientConfig::default(),
        diagnostics: DiagnosticsStore::new(),
    };
    assert_eq!(
        client.get_capabilities().window.unwrap().work_done_progress,
        Some(true)
    );
    client.start_response_listener().await.unwrap();
    assert_eq!(
        client.send_request("probe", None).await.unwrap(),
        serde_json::json!({"ok": true})
    );
    assert!(client.pending.is_loading().await);
    client.send_request("finish", None).await.unwrap();
    assert!(!client.pending.is_loading().await);
    client.process.kill().await.unwrap();
}
