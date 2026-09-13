//! The Claude client: `POST {base_url}/v1/messages` over HTTPS, streamed.

use std::time::Duration;

use futures_util::StreamExt;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use super::error::{ClaudeError, from_status};
use super::sse::{Assembler, SseDecoder};
use super::types::{Request, betas, body};
use crate::llm::auth::Credential;
use crate::llm::{Capabilities, ChatProvider, StreamEvent, StreamFuture};

pub const API_VERSION: &str = "2023-06-01";
/// The whole request, connection to the last body byte.
pub const TIMEOUT: Duration = Duration::from_secs(600);

/// The Claude client over HTTPS. One `reqwest::Client` for its lifetime; a request is
/// retried at most twice — after `backoff[0]`, then `backoff[1]` — on a rate limit, an
/// overload, a server error or a connection failure, and only before the first byte of
/// its stream was read. A started stream is never restarted.
#[derive(Debug, Clone)]
pub struct ClaudeClient {
    pub credential: Credential,
    pub base_url: String,
    pub backoff: [Duration; 2],
    http: reqwest::Client,
}

/// What one attempt ended in: a failure the next attempt may fix, or the final word.
enum Attempt {
    Retryable(ClaudeError),
    Final(ClaudeError),
}

impl ClaudeClient {
    pub fn new(credential: Credential) -> Self {
        let http = reqwest::Client::builder().timeout(TIMEOUT).build().expect("the HTTP client builds: no proxy or TLS setting of this app can fail");
        Self { credential, base_url: "https://api.anthropic.com".to_string(), backoff: [Duration::from_secs(1), Duration::from_secs(3)], http }
    }

    async fn run(&self, req: &Request, tx: mpsc::Sender<StreamEvent>, cancel: CancellationToken) -> Result<(), ClaudeError> {
        let body = body(req);
        let betas = betas(req, &self.credential).join(",");
        let mut attempt = 0;
        loop {
            match self.attempt(&body, &betas, &tx, &cancel).await {
                Ok(()) => return Ok(()),
                Err(Attempt::Retryable(e)) if attempt < self.backoff.len() && !cancel.is_cancelled() => {
                    tracing::warn!(error = %e, attempt, "request failed before its stream started; retrying");
                    tokio::select! {
                        _ = tokio::time::sleep(self.backoff[attempt]) => {}
                        _ = cancel.cancelled() => return Err(ClaudeError::Cancelled),
                    }
                    attempt += 1;
                }
                Err(Attempt::Retryable(e)) | Err(Attempt::Final(e)) => {
                    // the receiver may be gone; the error is returned either way
                    let _ = tx.send(StreamEvent::Error(e.to_string())).await;
                    return Err(e);
                }
            }
        }
    }

    async fn attempt(&self, body: &serde_json::Value, betas: &str, tx: &mpsc::Sender<StreamEvent>, cancel: &CancellationToken) -> Result<(), Attempt> {
        let mut request = self.http.post(format!("{}/v1/messages", self.base_url.trim_end_matches('/'))).header("anthropic-version", API_VERSION).json(body);
        request = match &self.credential {
            Credential::ApiKey(key) => request.header("x-api-key", key),
            Credential::Bearer(token) => request.bearer_auth(token),
        };
        if !betas.is_empty() {
            request = request.header("anthropic-beta", betas);
        }
        let response = tokio::select! {
            r = request.send() => r.map_err(|e| Attempt::Retryable(ClaudeError::Connection(e.to_string())))?,
            _ = cancel.cancelled() => return Err(Attempt::Final(ClaudeError::Cancelled)),
        };
        let status = response.status();
        if !status.is_success() {
            let headers = response.headers().clone();
            let text = response.text().await.map_err(|e| Attempt::Retryable(ClaudeError::Connection(e.to_string())))?;
            let error = from_status(status, &headers, &text);
            return Err(if error.is_retryable() { Attempt::Retryable(error) } else { Attempt::Final(error) });
        }

        let mut stream = response.bytes_stream();
        let mut decoder = SseDecoder::new();
        let mut assembler = Assembler::new();
        let mut consumed = false;
        loop {
            let chunk = tokio::select! {
                c = stream.next() => c,
                _ = cancel.cancelled() => return Err(Attempt::Final(ClaudeError::Cancelled)),
            };
            match chunk {
                Some(Ok(bytes)) => {
                    consumed = true;
                    for event in decoder.push(bytes.as_ref()).map_err(Attempt::Final)? {
                        for stream_event in assembler.feed(&event).map_err(Attempt::Final)? {
                            if tx.send(stream_event).await.is_err() {
                                return Err(Attempt::Final(ClaudeError::Cancelled));
                            }
                        }
                    }
                }
                Some(Err(e)) if consumed => return Err(Attempt::Final(ClaudeError::Stream(e.to_string()))),
                Some(Err(e)) => return Err(Attempt::Retryable(ClaudeError::Connection(e.to_string()))),
                None => break,
            }
        }
        decoder.finish().map_err(Attempt::Final)?;
        assembler.finish().map(|_| ()).map_err(Attempt::Final)
    }
}

impl ChatProvider for ClaudeClient {
    fn id(&self) -> &str {
        "claude"
    }
    fn capabilities(&self) -> Capabilities {
        Capabilities { tools: true, thinking: true, effort: true, cache_control: true }
    }
    fn stream<'a>(&'a self, req: &'a Request, tx: mpsc::Sender<StreamEvent>, cancel: CancellationToken) -> StreamFuture<'a> {
        Box::pin(self.run(req, tx, cancel))
    }
}
