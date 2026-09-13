//! The HTTP status to error mapping and the bounded retry, through a canned HTTP
//! server on a local port; and the OpenAI-compatible and Ollama adapters against
//! their recorded streams.

use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use satz_studio_core::llm::{
    ChatProvider, ClaudeClient, ClaudeError, ContentBlock, Credential, Effort, Message, Ollama,
    OpenAiCompat, Request, Role, StopReason, StreamEvent, SystemBlock, ToolDef, Usage,
};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

/// One response the server gives to one connection.
struct Canned {
    status: u16,
    headers: Vec<(&'static str, String)>,
    body: String,
}

impl Canned {
    fn json(status: u16, body: &str) -> Self {
        Self {
            status,
            headers: vec![("content-type", "application/json".to_string())],
            body: body.to_string(),
        }
    }
    fn stream(body: String) -> Self {
        Self {
            status: 200,
            headers: vec![("content-type", "text/event-stream".to_string())],
            body,
        }
    }
}

/// A server that answers `responses.len()` connections in order and keeps what each
/// request sent: the head (request line and headers) and the body.
struct Server {
    base_url: String,
    heads: Arc<Mutex<Vec<String>>>,
    bodies: Arc<Mutex<Vec<String>>>,
}

impl Server {
    fn start(responses: Vec<Canned>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("a local port");
        let base_url = format!("http://{}", listener.local_addr().expect("an address"));
        let heads = Arc::new(Mutex::new(Vec::new()));
        let bodies = Arc::new(Mutex::new(Vec::new()));
        let (h, b) = (heads.clone(), bodies.clone());
        std::thread::spawn(move || {
            for canned in responses {
                let (mut stream, _) = listener.accept().expect("a connection");
                let (head, body) = read_request(&mut stream);
                h.lock().unwrap().push(head);
                b.lock().unwrap().push(body);
                let mut response = format!(
                    "HTTP/1.1 {} {}\r\nconnection: close\r\ncontent-length: {}\r\n",
                    canned.status,
                    reason(canned.status),
                    canned.body.len()
                );
                for (name, value) in &canned.headers {
                    response.push_str(&format!("{name}: {value}\r\n"));
                }
                response.push_str("\r\n");
                response.push_str(&canned.body);
                stream.write_all(response.as_bytes()).expect("writes");
                stream.flush().expect("flushes");
            }
        });
        Self {
            base_url,
            heads,
            bodies,
        }
    }
    fn requests(&self) -> usize {
        self.heads.lock().unwrap().len()
    }
    fn head(&self, i: usize) -> String {
        self.heads.lock().unwrap()[i].to_lowercase()
    }
    fn body(&self, i: usize) -> serde_json::Value {
        serde_json::from_str(&self.bodies.lock().unwrap()[i]).expect("a JSON body")
    }
}

fn reason(status: u16) -> &'static str {
    match status {
        200 => "OK",
        400 => "Bad Request",
        401 => "Unauthorized",
        429 => "Too Many Requests",
        529 => "Overloaded",
        _ => "Status",
    }
}

fn read_request(stream: &mut std::net::TcpStream) -> (String, String) {
    let mut buf = Vec::new();
    let mut chunk = [0u8; 4096];
    let head_end = loop {
        let n = stream.read(&mut chunk).expect("reads");
        assert!(n > 0, "the client closed before the head ended");
        buf.extend_from_slice(&chunk[..n]);
        if let Some(pos) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
            break pos + 4;
        }
    };
    let head = String::from_utf8_lossy(&buf[..head_end]).to_string();
    let length: usize = head
        .lines()
        .find_map(|l| {
            l.to_lowercase()
                .strip_prefix("content-length:")
                .map(|v| v.trim().parse().expect("a length"))
        })
        .unwrap_or(0);
    while buf.len() < head_end + length {
        let n = stream.read(&mut chunk).expect("reads");
        assert!(n > 0, "the client closed before the body ended");
        buf.extend_from_slice(&chunk[..n]);
    }
    (
        head,
        String::from_utf8_lossy(&buf[head_end..head_end + length]).to_string(),
    )
}

fn fixture(dir: &str, name: &str) -> String {
    std::fs::read_to_string(format!(
        "{}/tests/fixtures/{dir}/{name}",
        env!("CARGO_MANIFEST_DIR")
    ))
    .expect("the recording exists")
}

fn request() -> Request {
    Request {
        model: "claude-opus-5".to_string(),
        max_tokens: 64_000,
        system: vec![SystemBlock {
            text: "the guide".to_string(),
            cache_control: None,
        }],
        messages: vec![Message {
            role: Role::User,
            content: vec![ContentBlock::text("hello")],
        }],
        tools: vec![ToolDef {
            name: "satz_transpile_check".to_string(),
            description: "Check an estate.".to_string(),
            input_schema: serde_json::json!({"type": "object", "properties": {"estate": {"type": "string"}}}),
            cache_control: None,
        }],
        effort: Effort::High,
        fallbacks: true,
    }
}

fn client(server: &Server, credential: Credential) -> ClaudeClient {
    let mut client = ClaudeClient::new(credential);
    client.base_url = server.base_url.clone();
    client.backoff = [Duration::from_millis(10), Duration::from_millis(10)];
    client
}

/// Stream a request; the events it sent and its result.
async fn stream(
    provider: &dyn ChatProvider,
    req: &Request,
) -> (Vec<StreamEvent>, Result<(), ClaudeError>) {
    let (tx, mut rx) = mpsc::channel(64);
    let (result, events) =
        tokio::join!(provider.stream(req, tx, CancellationToken::new()), async {
            let mut events = Vec::new();
            while let Some(e) = rx.recv().await {
                events.push(e);
            }
            events
        });
    (events, result)
}

const AUTH_ERROR: &str =
    r#"{"type":"error","error":{"type":"authentication_error","message":"invalid x-api-key"}}"#;

#[tokio::test]
async fn a_401_is_auth_and_is_not_retried() {
    let server = Server::start(vec![Canned::json(401, AUTH_ERROR)]);
    let (events, result) = stream(
        &client(&server, Credential::ApiKey("sk-example".to_string())),
        &request(),
    )
    .await;
    assert!(
        matches!(&result, Err(ClaudeError::Auth(m)) if m == "invalid x-api-key"),
        "{result:?}"
    );
    assert_eq!(server.requests(), 1);
    assert_eq!(
        events,
        vec![StreamEvent::Error(
            "authentication failed: invalid x-api-key".to_string()
        )]
    );
    let head = server.head(0);
    assert!(head.starts_with("post /v1/messages http/1.1"));
    assert!(head.contains("x-api-key: sk-example"));
    assert!(head.contains("anthropic-version: 2023-06-01"));
    assert!(head.contains("anthropic-beta: server-side-fallback-2026-07-01"));
    assert!(!head.contains("authorization:"));
    assert_eq!(server.body(0)["fallbacks"], "default");
}

#[tokio::test]
async fn a_bearer_credential_sends_the_oauth_beta() {
    let server = Server::start(vec![Canned::json(401, AUTH_ERROR)]);
    let mut req = request();
    req.fallbacks = false;
    let _ = stream(
        &client(&server, Credential::Bearer("tok-example".to_string())),
        &req,
    )
    .await;
    let head = server.head(0);
    assert!(head.contains("authorization: bearer tok-example"));
    assert!(head.contains("anthropic-beta: oauth-2025-04-20"));
    assert!(!head.contains("x-api-key"));
}

#[tokio::test]
async fn a_429_is_retried_twice_then_rate_limited_with_retry_after() {
    let limited = || Canned {
        status: 429,
        headers: vec![
            ("retry-after", "7".to_string()),
            ("content-type", "application/json".to_string()),
        ],
        body: r#"{"type":"error","error":{"type":"rate_limit_error","message":"slow down"}}"#
            .to_string(),
    };
    let server = Server::start(vec![limited(), limited(), limited()]);
    let (_, result) = stream(
        &client(&server, Credential::ApiKey("sk-example".to_string())),
        &request(),
    )
    .await;
    assert!(
        matches!(result, Err(ClaudeError::RateLimited { retry_after: Some(d) }) if d == Duration::from_secs(7)),
        "{result:?}"
    );
    assert_eq!(server.requests(), 3, "the first try and two retries");
}

#[tokio::test]
async fn a_529_is_overloaded_and_a_retry_that_succeeds_streams() {
    let overloaded = || {
        Canned::json(
            529,
            r#"{"type":"error","error":{"type":"overloaded_error","message":"Overloaded"}}"#,
        )
    };
    let server = Server::start(vec![overloaded(), overloaded(), overloaded()]);
    let (_, result) = stream(
        &client(&server, Credential::ApiKey("sk-example".to_string())),
        &request(),
    )
    .await;
    assert!(matches!(result, Err(ClaudeError::Overloaded)));
    assert_eq!(server.requests(), 3);

    let server = Server::start(vec![
        overloaded(),
        Canned::stream(fixture("sse", "text_turn.sse")),
    ]);
    let (events, result) = stream(
        &client(&server, Credential::ApiKey("sk-example".to_string())),
        &request(),
    )
    .await;
    result.expect("the retry streams");
    assert_eq!(server.requests(), 2);
    assert!(
        matches!(events.first(), Some(StreamEvent::Started { id, .. }) if id == "msg_01TextTurnExample")
    );
    assert!(matches!(
        events.last(),
        Some(StreamEvent::Done {
            stop_reason: StopReason::EndTurn,
            ..
        })
    ));
}

#[tokio::test]
async fn a_stream_that_starts_then_errors_is_not_retried() {
    let server = Server::start(vec![
        Canned::stream(fixture("sse", "mid_stream_error.sse")),
        Canned::stream(fixture("sse", "text_turn.sse")),
    ]);
    let (events, result) = stream(
        &client(&server, Credential::ApiKey("sk-example".to_string())),
        &request(),
    )
    .await;
    assert!(
        matches!(&result, Err(ClaudeError::Stream(m)) if m == "overloaded_error: Overloaded"),
        "{result:?}"
    );
    assert_eq!(server.requests(), 1, "a started stream is never restarted");
    assert_eq!(
        events,
        vec![
            StreamEvent::Started {
                id: "msg_01ErrorExample".to_string(),
                model: "claude-opus-5".to_string()
            },
            StreamEvent::TextDelta("Looking".to_string()),
            StreamEvent::Error("stream: overloaded_error: Overloaded".to_string()),
        ]
    );
}

#[tokio::test]
async fn a_400_carries_the_message_and_the_request_id() {
    let server = Server::start(vec![Canned {
        status: 400,
        headers: vec![("request-id", "req_example".to_string()), ("content-type", "application/json".to_string())],
        body: r#"{"type":"error","error":{"type":"invalid_request_error","message":"messages: final assistant content cannot end with trailing whitespace"}}"#.to_string(),
    }]);
    let (_, result) = stream(
        &client(&server, Credential::ApiKey("sk-example".to_string())),
        &request(),
    )
    .await;
    assert!(
        matches!(&result, Err(ClaudeError::BadRequest { message, request_id: Some(id) }) if message.starts_with("messages:") && id == "req_example"),
        "{result:?}"
    );
    assert_eq!(server.requests(), 1);
}

#[tokio::test]
async fn a_connection_failure_is_retried_then_reported() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("a port");
    let base_url = format!("http://{}", listener.local_addr().unwrap());
    drop(listener);
    let mut client = ClaudeClient::new(Credential::ApiKey("sk-example".to_string()));
    client.base_url = base_url;
    client.backoff = [Duration::from_millis(5), Duration::from_millis(5)];
    let (_, result) = stream(&client, &request()).await;
    assert!(
        matches!(result, Err(ClaudeError::Connection(_))),
        "{result:?}"
    );
}

/// A transcript with one tool exchange, as both adapters must map it.
fn tool_exchange() -> Request {
    let mut req = request();
    req.system.push(SystemBlock {
        text: "estate: /estates/acme".to_string(),
        cache_control: None,
    });
    req.messages = vec![
        Message {
            role: Role::User,
            content: vec![ContentBlock::text("check it")],
        },
        Message {
            role: Role::Assistant,
            content: vec![
                ContentBlock::Thinking {
                    thinking: "dropped".to_string(),
                    signature: "sig".to_string(),
                },
                ContentBlock::text("Checking."),
                ContentBlock::ToolUse {
                    id: "call_example1".to_string(),
                    name: "satz_transpile_check".to_string(),
                    input: serde_json::json!({"estate": "estates/acme.satz"}),
                },
            ],
        },
        Message {
            role: Role::User,
            content: vec![
                ContentBlock::ToolResult {
                    tool_use_id: "call_example1".to_string(),
                    content: "ok".to_string(),
                    is_error: true,
                    cache_control: None,
                },
                ContentBlock::text("and now?"),
            ],
        },
    ];
    req
}

#[tokio::test]
async fn the_openai_adapter_maps_the_request_and_its_stream() {
    let server = Server::start(vec![Canned::stream(fixture("llm", "openai_stream.sse"))]);
    let provider = OpenAiCompat::new(
        server.base_url.clone(),
        Some("key-example".to_string()),
        "local-model",
    );
    assert!(
        !provider.capabilities().thinking
            && !provider.capabilities().effort
            && !provider.capabilities().cache_control
            && provider.capabilities().tools
    );
    let (events, result) = stream(&provider, &tool_exchange()).await;
    result.expect("streams");

    let head = server.head(0);
    assert!(head.starts_with("post /chat/completions http/1.1"));
    assert!(head.contains("authorization: bearer key-example"));
    let body = server.body(0);
    assert_eq!(body["model"], "local-model");
    assert_eq!(body["stream"], true);
    assert_eq!(
        body["messages"][0],
        serde_json::json!({"role": "system", "content": "the guide\n\nestate: /estates/acme"})
    );
    assert_eq!(
        body["messages"][1],
        serde_json::json!({"role": "user", "content": "check it"})
    );
    assert_eq!(body["messages"][2]["role"], "assistant");
    assert_eq!(body["messages"][2]["content"], "Checking.");
    assert_eq!(
        body["messages"][2]["tool_calls"][0]["function"]["name"],
        "satz_transpile_check"
    );
    assert_eq!(
        body["messages"][2]["tool_calls"][0]["function"]["arguments"],
        "{\"estate\":\"estates/acme.satz\"}"
    );
    assert_eq!(
        body["messages"][3],
        serde_json::json!({"role": "tool", "tool_call_id": "call_example1", "content": "error: ok"})
    );
    assert_eq!(
        body["messages"][4],
        serde_json::json!({"role": "user", "content": "and now?"})
    );
    assert_eq!(body["tools"][0]["type"], "function");
    assert_eq!(body["tools"][0]["function"]["name"], "satz_transpile_check");
    assert_eq!(body["tools"][0]["function"]["parameters"]["type"], "object");
    assert!(!body.to_string().contains("thinking"));

    assert_eq!(
        events,
        vec![
            StreamEvent::Started {
                id: "chatcmpl-example".to_string(),
                model: "local-model".to_string()
            },
            StreamEvent::TextDelta("Checking.".to_string()),
            StreamEvent::BlockStop {
                index: 0,
                block: ContentBlock::text("Checking.")
            },
            StreamEvent::ToolUseStart {
                index: 1,
                id: "call_example1".to_string(),
                name: "satz_transpile_check".to_string()
            },
            StreamEvent::ToolInputDelta {
                index: 1,
                partial_json: "{\"estate\": ".to_string()
            },
            StreamEvent::ToolInputDelta {
                index: 1,
                partial_json: "\"estates/acme.satz\"}".to_string()
            },
            StreamEvent::BlockStop {
                index: 1,
                block: ContentBlock::ToolUse {
                    id: "call_example1".to_string(),
                    name: "satz_transpile_check".to_string(),
                    input: serde_json::json!({"estate": "estates/acme.satz"})
                }
            },
            StreamEvent::Done {
                stop_reason: StopReason::ToolUse,
                stop_details: None,
                usage: Usage {
                    input_tokens: 120,
                    output_tokens: 18,
                    cache_creation_input_tokens: None,
                    cache_read_input_tokens: None
                }
            },
        ]
    );
}

#[tokio::test]
async fn the_ollama_adapter_maps_the_request_and_its_stream() {
    let server = Server::start(vec![Canned {
        status: 200,
        headers: vec![("content-type", "application/x-ndjson".to_string())],
        body: fixture("llm", "ollama_stream.ndjson"),
    }]);
    let provider = Ollama::new(server.base_url.clone(), "local-model");
    let (events, result) = stream(&provider, &tool_exchange()).await;
    result.expect("streams");

    assert!(server.head(0).starts_with("post /api/chat http/1.1"));
    let body = server.body(0);
    assert_eq!(body["model"], "local-model");
    assert_eq!(
        body["messages"][2]["tool_calls"][0],
        serde_json::json!({"function": {"name": "satz_transpile_check", "arguments": {"estate": "estates/acme.satz"}}})
    );
    assert_eq!(
        body["messages"][3],
        serde_json::json!({"role": "tool", "content": "error: ok", "tool_name": "satz_transpile_check"})
    );
    assert_eq!(body["tools"][0]["function"]["name"], "satz_transpile_check");

    assert_eq!(
        events,
        vec![
            StreamEvent::Started {
                id: "ollama".to_string(),
                model: "local-model".to_string()
            },
            StreamEvent::TextDelta("Checking.".to_string()),
            StreamEvent::BlockStop {
                index: 0,
                block: ContentBlock::text("Checking.")
            },
            StreamEvent::ToolUseStart {
                index: 1,
                id: "call_0".to_string(),
                name: "satz_transpile_check".to_string()
            },
            StreamEvent::ToolInputDelta {
                index: 1,
                partial_json: "{\"estate\":\"estates/acme.satz\"}".to_string()
            },
            StreamEvent::BlockStop {
                index: 1,
                block: ContentBlock::ToolUse {
                    id: "call_0".to_string(),
                    name: "satz_transpile_check".to_string(),
                    input: serde_json::json!({"estate": "estates/acme.satz"})
                }
            },
            StreamEvent::Done {
                stop_reason: StopReason::ToolUse,
                stop_details: None,
                usage: Usage {
                    input_tokens: 98,
                    output_tokens: 14,
                    cache_creation_input_tokens: None,
                    cache_read_input_tokens: None
                }
            },
        ]
    );
}

#[tokio::test]
async fn an_adapter_maps_a_status_error_too() {
    let server = Server::start(vec![Canned::json(
        401,
        r#"{"error":{"message":"bad key","type":"invalid_request_error"}}"#,
    )]);
    let provider = OpenAiCompat::new(server.base_url.clone(), None, "local-model");
    let (events, result) = stream(&provider, &request()).await;
    assert!(
        matches!(&result, Err(ClaudeError::Auth(m)) if m == "bad key"),
        "{result:?}"
    );
    assert_eq!(
        events,
        vec![StreamEvent::Error(
            "authentication failed: bad key".to_string()
        )]
    );
    assert!(
        !server.head(0).contains("authorization:"),
        "no key, no header"
    );
}
