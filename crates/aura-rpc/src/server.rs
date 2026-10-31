//! Small bounded HTTP/WebSocket transport for the JSON-RPC service.

use std::io::{self, Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream, ToSocketAddrs};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64;
use serde_json::{Value, json};
use sha1::{Digest, Sha1};

use crate::{HttpExtension, HttpExtensionResponse, RpcError, RpcService, SubscriptionPoll};

/// Maximum accepted HTTP JSON body size.
pub const MAX_HTTP_BODY: usize = 1_048_576;
/// Maximum accepted WebSocket text or binary message size.
pub const MAX_WS_MESSAGE: usize = 1_048_576;
const MAX_HTTP_HEADERS: usize = 32_768;
const MAX_CONNECTIONS: usize = 128;
const WS_GUID: &str = "258EAFA5-E914-47DA-95CA-C5AB0DC85B11";
static NEXT_SUBSCRIPTION: AtomicU64 = AtomicU64::new(1);

/// Running RPC listener. Dropping or shutting it down stops accepting sockets.
pub struct RpcServerHandle {
    addr: SocketAddr,
    stop: Arc<AtomicBool>,
    join: Option<JoinHandle<()>>,
}

impl RpcServerHandle {
    /// Bind and start a JSON-RPC listener on the requested socket address.
    pub fn spawn<A: ToSocketAddrs>(addr: A, service: RpcService) -> io::Result<Self> {
        Self::spawn_with_extension(addr, service, None)
    }

    /// Bind and start JSON-RPC plus an optional bounded HTTP route extension.
    pub fn spawn_with_extension<A: ToSocketAddrs>(
        addr: A,
        service: RpcService,
        extension: Option<Arc<dyn HttpExtension>>,
    ) -> io::Result<Self> {
        let listener = TcpListener::bind(addr)?;
        let addr = listener.local_addr()?;
        listener.set_nonblocking(true)?;
        let stop = Arc::new(AtomicBool::new(false));
        let worker_count = Arc::new(AtomicUsize::new(0));
        let thread_stop = Arc::clone(&stop);
        let join = thread::Builder::new()
            .name("aura-rpc-listener".to_owned())
            .spawn(move || {
                while !thread_stop.load(Ordering::Acquire) {
                    match listener.accept() {
                        Ok((stream, _peer)) => {
                            if worker_count.fetch_add(1, Ordering::AcqRel) >= MAX_CONNECTIONS {
                                worker_count.fetch_sub(1, Ordering::AcqRel);
                                let _ = write_http_error(stream, 503, "Service Unavailable");
                                continue;
                            }
                            let service = service.clone();
                            let extension = extension.clone();
                            let count = Arc::clone(&worker_count);
                            let guard_count = Arc::clone(&count);
                            if thread::Builder::new()
                                .name("aura-rpc-connection".to_owned())
                                .spawn(move || {
                                    let _guard = WorkerGuard(guard_count);
                                    let _ = handle_connection(stream, service, extension);
                                })
                                .is_err()
                            {
                                count.fetch_sub(1, Ordering::AcqRel);
                            }
                        }
                        Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                            thread::sleep(Duration::from_millis(10));
                        }
                        Err(_) => thread::sleep(Duration::from_millis(50)),
                    }
                }
            })?;
        Ok(Self {
            addr,
            stop,
            join: Some(join),
        })
    }

    /// Address the listener actually bound to (useful with port zero in tests).
    pub fn local_addr(&self) -> SocketAddr {
        self.addr
    }

    /// Stop accepting new connections and join the listener thread.
    pub fn shutdown(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
}

impl Drop for RpcServerHandle {
    fn drop(&mut self) {
        self.shutdown();
    }
}

struct WorkerGuard(Arc<AtomicUsize>);
impl Drop for WorkerGuard {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::AcqRel);
    }
}

struct HttpRequest {
    method: String,
    path: String,
    headers: Vec<(String, String)>,
    body: Vec<u8>,
}

impl HttpRequest {
    fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(key, _)| key.eq_ignore_ascii_case(name))
            .map(|(_, value)| value.as_str())
    }
}

fn handle_connection(
    mut stream: TcpStream,
    service: RpcService,
    extension: Option<Arc<dyn HttpExtension>>,
) -> io::Result<()> {
    stream.set_nodelay(true)?;
    stream.set_read_timeout(Some(Duration::from_secs(15)))?;
    stream.set_write_timeout(Some(Duration::from_secs(15)))?;
    let request = match read_http_request(&mut stream) {
        Ok(request) => request,
        Err(status) => return write_http_error(stream, status, reason(status)),
    };
    let upgrade = request
        .header("upgrade")
        .is_some_and(|value| value.eq_ignore_ascii_case("websocket"));
    if upgrade {
        if request.path != "/ws" {
            return write_http_error(stream, 404, "Not Found");
        }
        return serve_websocket(stream, &request, service);
    }
    if let Some(extension) = extension
        && let Some(HttpExtensionResponse {
            status,
            content_type,
            body,
        }) = extension.handle(
            &request.method,
            &request.path,
            request.header("content-type"),
            &request.body,
        )
    {
        let status = if matches!(
            status,
            200 | 201 | 204 | 400 | 404 | 405 | 406 | 415 | 429 | 500 | 501 | 503
        ) {
            status
        } else {
            500
        };
        if body.len() > MAX_HTTP_BODY {
            return write_http_error(stream, 500, "Internal Server Error");
        }
        return write_http_response(stream, status, reason(status), &content_type, &body);
    }
    if request.method != "POST" {
        return write_http_error(stream, 405, "Method Not Allowed");
    }
    if request.path != "/" && request.path != "/rpc" {
        return write_http_error(stream, 404, "Not Found");
    }
    if !request
        .header("content-type")
        .is_some_and(|value| value.to_ascii_lowercase().starts_with("application/json"))
    {
        return write_http_error(stream, 415, "Unsupported Media Type");
    }
    match service.process_http_message(&request.body) {
        Some(body) => write_http_response(stream, 200, "OK", "application/json", &body),
        None => write_http_response(stream, 204, "No Content", "application/json", b""),
    }
}

fn read_http_request(stream: &mut impl Read) -> Result<HttpRequest, u16> {
    let mut bytes = Vec::with_capacity(4096);
    let mut chunk = [0u8; 1024];
    let header_end = loop {
        if let Some(index) = bytes.windows(4).position(|part| part == b"\r\n\r\n") {
            break index + 4;
        }
        if bytes.len() >= MAX_HTTP_HEADERS {
            return Err(431);
        }
        let count = stream.read(&mut chunk).map_err(|_| 400u16)?;
        if count == 0 {
            return Err(400);
        }
        bytes.extend_from_slice(&chunk[..count]);
    };
    if header_end > MAX_HTTP_HEADERS {
        return Err(431);
    }
    let header_text = std::str::from_utf8(&bytes[..header_end]).map_err(|_| 400u16)?;
    let mut lines = header_text.split("\r\n");
    let request_line = lines.next().ok_or(400u16)?;
    let mut parts = request_line.split_ascii_whitespace();
    let method = parts.next().ok_or(400u16)?.to_owned();
    let path = parts.next().ok_or(400u16)?.to_owned();
    let version = parts.next().ok_or(400u16)?;
    if parts.next().is_some() || (version != "HTTP/1.1" && version != "HTTP/1.0") {
        return Err(400);
    }
    let mut headers = Vec::new();
    let mut content_length = None;
    for line in lines {
        if line.is_empty() {
            break;
        }
        if line.starts_with([' ', '\t']) {
            return Err(400);
        }
        let (key, value) = line.split_once(':').ok_or(400u16)?;
        let key = key.trim();
        if key.is_empty() || !key.bytes().all(is_token) {
            return Err(400);
        }
        let value = value.trim();
        if key.eq_ignore_ascii_case("transfer-encoding") {
            return Err(400);
        }
        if key.eq_ignore_ascii_case("content-length") {
            if content_length.is_some() {
                return Err(400);
            }
            content_length = Some(value.parse::<usize>().map_err(|_| 400u16)?);
        }
        headers.push((key.to_owned(), value.to_owned()));
    }
    let length = content_length.unwrap_or(0);
    if length > MAX_HTTP_BODY {
        return Err(413);
    }
    let mut body = bytes[header_end..].to_vec();
    if body.len() > length {
        return Err(400);
    }
    while body.len() < length {
        let remaining = length - body.len();
        let take = remaining.min(chunk.len());
        let count = stream.read(&mut chunk[..take]).map_err(|_| 400u16)?;
        if count == 0 {
            return Err(400);
        }
        body.extend_from_slice(&chunk[..count]);
    }
    Ok(HttpRequest {
        method,
        path,
        headers,
        body,
    })
}

fn is_token(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || b"!#$%&'*+-.^_`|~".contains(&byte)
}

fn write_http_response(
    mut stream: TcpStream,
    status: u16,
    phrase: &str,
    content_type: &str,
    body: &[u8],
) -> io::Result<()> {
    write!(
        stream,
        "HTTP/1.1 {status} {phrase}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    )?;
    stream.write_all(body)?;
    stream.flush()
}

fn write_http_error(stream: TcpStream, status: u16, phrase: &str) -> io::Result<()> {
    write_http_response(
        stream,
        status,
        phrase,
        "text/plain; charset=utf-8",
        phrase.as_bytes(),
    )
}

fn reason(status: u16) -> &'static str {
    match status {
        200 => "OK",
        201 => "Created",
        204 => "No Content",
        400 => "Bad Request",
        404 => "Not Found",
        405 => "Method Not Allowed",
        406 => "Not Acceptable",
        413 => "Payload Too Large",
        415 => "Unsupported Media Type",
        429 => "Too Many Requests",
        431 => "Request Header Fields Too Large",
        500 => "Internal Server Error",
        501 => "Not Implemented",
        503 => "Service Unavailable",
        _ => "Bad Request",
    }
}

fn serve_websocket(
    mut stream: TcpStream,
    request: &HttpRequest,
    service: RpcService,
) -> io::Result<()> {
    if request.method != "GET" {
        return write_http_error(stream, 405, "Method Not Allowed");
    }
    if !request.header("connection").is_some_and(|value| {
        value
            .split(',')
            .any(|token| token.trim().eq_ignore_ascii_case("upgrade"))
    }) {
        return write_http_error(stream, 400, "Invalid WebSocket Connection Header");
    }
    if request.header("sec-websocket-version") != Some("13") {
        return write_http_error(stream, 400, "Bad WebSocket Version");
    }
    let Some(key) = request.header("sec-websocket-key") else {
        return write_http_error(stream, 400, "Missing WebSocket Key");
    };
    let decoded = match BASE64.decode(key) {
        Ok(value) if value.len() == 16 => value,
        _ => return write_http_error(stream, 400, "Invalid WebSocket Key"),
    };
    let _ = decoded;
    let mut digest = Sha1::new();
    digest.update(key.as_bytes());
    digest.update(WS_GUID.as_bytes());
    let accept = BASE64.encode(digest.finalize());
    write!(
        stream,
        "HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Accept: {accept}\r\n\r\n"
    )?;
    stream.flush()?;
    stream.set_read_timeout(None)?;
    serve_websocket_frames(stream, service)
}

fn serve_websocket_frames(stream: TcpStream, service: RpcService) -> io::Result<()> {
    let reader = stream.try_clone()?;
    let writer = Arc::new(Mutex::new(stream));
    let closed = Arc::new(AtomicBool::new(false));
    while !closed.load(Ordering::Acquire) {
        let frame = match read_client_frame(&mut &reader) {
            Ok(frame) => frame,
            Err(error)
                if error.kind() == io::ErrorKind::UnexpectedEof
                    || error.kind() == io::ErrorKind::ConnectionReset =>
            {
                break;
            }
            Err(error) => return Err(error),
        };
        match frame.opcode {
            0x8 => {
                let mut guard = writer.lock().unwrap_or_else(|e| e.into_inner());
                let _ = write_server_frame(&mut *guard, 0x8, &frame.payload);
                break;
            }
            0x9 => {
                let mut guard = writer.lock().unwrap_or_else(|e| e.into_inner());
                write_server_frame(&mut *guard, 0xA, &frame.payload)?;
            }
            0x1 => {
                let response = handle_websocket_message(&service, &frame.payload, &writer, &closed);
                if let Some(body) = response {
                    let mut guard = writer.lock().unwrap_or_else(|e| e.into_inner());
                    write_server_frame(&mut *guard, 0x1, &body)?;
                }
            }
            0x2 => {
                send_close(&writer, 1003, "text only")?;
                break;
            }
            _ => {
                send_close(&writer, 1002, "unsupported frame")?;
                break;
            }
        }
    }
    closed.store(true, Ordering::Release);
    Ok(())
}

fn handle_websocket_message(
    service: &RpcService,
    payload: &[u8],
    writer: &Arc<Mutex<TcpStream>>,
    closed: &Arc<AtomicBool>,
) -> Option<Vec<u8>> {
    if payload.len() > MAX_WS_MESSAGE {
        let _ = send_close(writer, 1009, "message too large");
        closed.store(true, Ordering::Release);
        return None;
    }
    let value = match serde_json::from_slice::<Value>(payload) {
        Ok(value) => value,
        Err(_) => return service.process_websocket_message(payload),
    };
    let Some(object) = value.as_object() else {
        return service.process_websocket_message(payload);
    };
    if object.get("method").and_then(Value::as_str) != Some("subscribe") {
        return service.process_websocket_message(payload);
    }
    let id_present = object.contains_key("id");
    let id = object.get("id").cloned().unwrap_or(Value::Null);
    let valid = object.get("jsonrpc") == Some(&Value::String("2.0".to_owned()))
        && (id.is_null() || id.is_string() || id.as_i64().is_some() || id.as_u64().is_some())
        && object
            .keys()
            .all(|key| ["jsonrpc", "id", "method", "params"].contains(&key.as_str()));
    if !valid {
        return Some(serde_json::to_vec(&json!({"jsonrpc":"2.0","error":{"code":-32600,"message":"Invalid Request"},"id":null})).unwrap_or_default());
    }
    let mut subscription_source = match service.backend().subscribe() {
        Ok(receiver) => receiver,
        Err(error) => {
            return Some(serde_json::to_vec(&error_response_for_ws(id, error)).unwrap_or_default());
        }
    };
    let subscription = format!("sub-{}", NEXT_SUBSCRIPTION.fetch_add(1, Ordering::Relaxed));
    let ack = if id_present {
        Some(json!({"jsonrpc":"2.0","result":{"subscription":subscription},"id":id}))
    } else {
        None
    };
    let writer = Arc::clone(writer);
    let closed = Arc::clone(closed);
    let subscription_for_thread = subscription.clone();
    let _ = thread::Builder::new().name("aura-rpc-subscription".to_owned()).spawn(move || {
        while !closed.load(Ordering::Acquire) {
            match subscription_source.poll(Duration::from_millis(250)) {
                SubscriptionPoll::Event(event) => {
                    let notification = serde_json::to_vec(&json!({"jsonrpc":"2.0","method":"subscription","params":{"subscription":subscription_for_thread,"result":event}})).unwrap_or_default();
                    let mut guard = writer.lock().unwrap_or_else(|e| e.into_inner());
                    if write_server_frame(&mut *guard, 0x1, &notification).is_err() { break; }
                }
                SubscriptionPoll::Timeout => continue,
                SubscriptionPoll::Closed => break,
            }
        }
    });
    ack.map(|value| serde_json::to_vec(&value).unwrap_or_default())
}

fn error_response_for_ws(id: Value, error: RpcError) -> Value {
    let mut entry = serde_json::Map::new();
    entry.insert("code".to_owned(), Value::from(error.code));
    entry.insert("message".to_owned(), Value::from(error.message));
    if let Some(data) = error.data {
        entry.insert("data".to_owned(), data);
    }
    json!({"jsonrpc":"2.0","error":Value::Object(entry),"id":id})
}

fn send_close(writer: &Arc<Mutex<TcpStream>>, code: u16, reason: &str) -> io::Result<()> {
    let mut payload = code.to_be_bytes().to_vec();
    payload.extend_from_slice(reason.as_bytes());
    let mut guard = writer.lock().unwrap_or_else(|e| e.into_inner());
    write_server_frame(&mut *guard, 0x8, &payload)
}

#[derive(Debug)]
struct Frame {
    opcode: u8,
    payload: Vec<u8>,
}

fn read_client_frame(stream: &mut impl Read) -> io::Result<Frame> {
    let mut head = [0u8; 2];
    stream.read_exact(&mut head)?;
    let fin = head[0] & 0x80 != 0;
    let opcode = head[0] & 0x0f;
    let masked = head[1] & 0x80 != 0;
    if !fin || head[0] & 0x70 != 0 || !masked {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "invalid WebSocket frame",
        ));
    }
    let mut length = u64::from(head[1] & 0x7f);
    if length == 126 {
        let mut ext = [0u8; 2];
        stream.read_exact(&mut ext)?;
        length = u16::from_be_bytes(ext) as u64;
        if length < 126 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "nonminimal frame length",
            ));
        }
    } else if length == 127 {
        let mut ext = [0u8; 8];
        stream.read_exact(&mut ext)?;
        length = u64::from_be_bytes(ext);
        if length < 65_536 || length >> 63 != 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "invalid frame length",
            ));
        }
    }
    if length > MAX_WS_MESSAGE as u64 || (opcode >= 0x8 && length > 125) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "frame too large",
        ));
    }
    let mut mask = [0u8; 4];
    stream.read_exact(&mut mask)?;
    let mut payload = vec![0u8; length as usize];
    stream.read_exact(&mut payload)?;
    for (index, byte) in payload.iter_mut().enumerate() {
        *byte ^= mask[index % 4];
    }
    Ok(Frame { opcode, payload })
}

fn write_server_frame(stream: &mut impl Write, opcode: u8, payload: &[u8]) -> io::Result<()> {
    if payload.len() > MAX_WS_MESSAGE {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "frame too large",
        ));
    }
    stream.write_all(&[0x80 | opcode])?;
    match payload.len() {
        0..=125 => stream.write_all(&[payload.len() as u8])?,
        126..=65_535 => {
            stream.write_all(&[126])?;
            stream.write_all(&(payload.len() as u16).to_be_bytes())?;
        }
        _ => {
            stream.write_all(&[127])?;
            stream.write_all(&(payload.len() as u64).to_be_bytes())?;
        }
    }
    stream.write_all(payload)?;
    stream.flush()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{RpcBackend, RpcError};
    use std::io::Cursor;
    use std::sync::mpsc;

    #[test]
    fn websocket_accept_matches_rfc6455_example() {
        let mut digest = Sha1::new();
        digest.update(b"dGhlIHNhbXBsZSBub25jZQ==");
        digest.update(WS_GUID.as_bytes());
        assert_eq!(
            BASE64.encode(digest.finalize()),
            "s3pPLMBiTxaQ9kYGzzhZRbK+xOo="
        );
    }

    #[test]
    fn server_frame_length_encoding_roundtrips_through_reader() {
        for size in [0usize, 125, 126, 65_535, 65_536] {
            let mut bytes = Vec::new();
            write_server_frame(&mut bytes, 1, &vec![b'x'; size]).unwrap();
            assert_eq!(bytes[0], 0x81);
            assert!(bytes.len() >= size + 2);
        }
    }

    #[test]
    fn http_rejects_oversize_body_before_reading_payload() {
        let request = format!(
            "POST /rpc HTTP/1.1\r\nHost: localhost\r\nContent-Length: {}\r\n\r\n",
            MAX_HTTP_BODY + 1
        );
        let result = read_http_request(&mut Cursor::new(request.into_bytes()));
        assert!(matches!(result, Err(413)));
    }

    #[test]
    fn client_websocket_frames_must_be_masked() {
        let mut reader = Cursor::new(vec![0x81, 0x01, b'x']);
        let result = read_client_frame(&mut reader);
        assert_eq!(result.unwrap_err().kind(), io::ErrorKind::InvalidData);
    }

    #[derive(Default)]
    struct MockBackend {
        senders: Mutex<Vec<mpsc::Sender<Value>>>,
    }

    impl MockBackend {
        fn publish(&self, value: Value) {
            for sender in self.senders.lock().unwrap().iter() {
                let _ = sender.send(value.clone());
            }
        }
    }

    impl RpcBackend for MockBackend {
        fn submit_tx(&self, _: &Value) -> Result<Value, RpcError> {
            Ok(json!("0x01"))
        }
        fn get_balance(&self, _: &Value) -> Result<Value, RpcError> {
            Ok(json!({"balance":"5","nonce":0}))
        }
        fn get_tx(&self, _: &Value) -> Result<Value, RpcError> {
            Ok(Value::Null)
        }
        fn get_block(&self, _: &Value) -> Result<Value, RpcError> {
            Ok(json!({"height":0}))
        }
        fn call(&self, _: &Value) -> Result<Value, RpcError> {
            Err(RpcError::new(-32004, "unsupported"))
        }
        fn estimate_gas(&self, _: &Value) -> Result<Value, RpcError> {
            Ok(json!({"gasUnits":21000}))
        }
        fn subscribe(&self) -> Result<Box<dyn crate::RpcSubscription>, RpcError> {
            let (sender, receiver) = mpsc::channel();
            self.senders.lock().unwrap().push(sender);
            Ok(Box::new(receiver))
        }
    }

    #[test]
    fn http_server_returns_json_rpc_response() {
        let backend = Arc::new(MockBackend::default());
        let service = RpcService::new(backend);
        let server = RpcServerHandle::spawn("127.0.0.1:0", service).unwrap();
        let mut client = TcpStream::connect(server.local_addr()).unwrap();
        let body = br#"{"jsonrpc":"2.0","id":5,"method":"get_block"}"#;
        write!(client, "POST /rpc HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", body.len()).unwrap();
        client.write_all(body).unwrap();
        let mut response = Vec::new();
        client.read_to_end(&mut response).unwrap();
        let response = String::from_utf8(response).unwrap();
        assert!(response.starts_with("HTTP/1.1 200 OK"));
        assert!(response.contains(r#""result":{"height":0}"#));
        drop(server);
    }

    #[test]
    fn websocket_subscription_acknowledges_and_streams_commit_events() {
        let backend = Arc::new(MockBackend::default());
        let server =
            RpcServerHandle::spawn("127.0.0.1:0", RpcService::new(backend.clone())).unwrap();
        let mut client = TcpStream::connect(server.local_addr()).unwrap();
        client
            .set_read_timeout(Some(Duration::from_secs(3)))
            .unwrap();
        client.write_all(b"GET /ws HTTP/1.1\r\nHost: localhost\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Version: 13\r\nSec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\n\r\n").unwrap();
        let mut handshake = Vec::new();
        let mut byte = [0u8; 1];
        while !handshake.ends_with(b"\r\n\r\n") {
            client.read_exact(&mut byte).unwrap();
            handshake.push(byte[0]);
        }
        let handshake = String::from_utf8(handshake).unwrap();
        assert!(handshake.starts_with("HTTP/1.1 101"));
        assert!(handshake.contains("Sec-WebSocket-Accept: s3pPLMBiTxaQ9kYGzzhZRbK+xOo="));

        write_client_text(
            &mut client,
            br#"{"jsonrpc":"2.0","id":1,"method":"subscribe","params":{}}"#,
        );
        let ack: Value = serde_json::from_slice(&read_server_text(&mut client)).unwrap();
        let subscription = ack["result"]["subscription"].as_str().unwrap().to_owned();
        assert_eq!(ack["id"], 1);
        backend.publish(json!({"height":9}));
        let event: Value = serde_json::from_slice(&read_server_text(&mut client)).unwrap();
        assert_eq!(event["method"], "subscription");
        assert_eq!(event["params"]["subscription"], subscription);
        assert_eq!(event["params"]["result"]["height"], 9);

        write_client_frame(&mut client, 0x8, &1000u16.to_be_bytes());
        drop(server);
    }

    fn write_client_text(stream: &mut TcpStream, payload: &[u8]) {
        write_client_frame(stream, 0x1, payload);
    }

    fn write_client_frame(stream: &mut TcpStream, opcode: u8, payload: &[u8]) {
        assert!(payload.len() < 126);
        let mask = [0x12, 0x34, 0x56, 0x78];
        stream
            .write_all(&[0x80 | opcode, 0x80 | payload.len() as u8])
            .unwrap();
        stream.write_all(&mask).unwrap();
        let masked = payload
            .iter()
            .enumerate()
            .map(|(i, byte)| byte ^ mask[i % 4])
            .collect::<Vec<_>>();
        stream.write_all(&masked).unwrap();
    }

    fn read_server_text(stream: &mut TcpStream) -> Vec<u8> {
        let mut head = [0u8; 2];
        stream.read_exact(&mut head).unwrap();
        assert_eq!(head[0], 0x81);
        assert_eq!(head[1] & 0x80, 0);
        let length = match head[1] {
            0..=125 => head[1] as usize,
            126 => {
                let mut x = [0; 2];
                stream.read_exact(&mut x).unwrap();
                u16::from_be_bytes(x) as usize
            }
            _ => panic!("unexpected payload length"),
        };
        let mut payload = vec![0; length];
        stream.read_exact(&mut payload).unwrap();
        payload
    }
}
