//! A fake Amazon Bedrock for the AWS SDK: the SDK configuration that points
//! at it, the Converse responses and event-stream frames it answers with,
//! and a server that sends a ConverseStream response over time.

use std::time::Duration;

use aws_sdk_bedrockruntime::config::retry::RetryConfig;
use aws_sdk_bedrockruntime::config::{BehaviorVersion, Credentials, Region};
use aws_sdk_bedrockruntime::types::{ContentBlock, ConversationRole, Message};
use aws_smithy_types::event_stream::{Header, HeaderValue, Message as EventMessage};
use aws_smithy_types::{Document, Number};
use langfuse_rust_sdk::bedrock::Bedrock;
use langfuse_rust_sdk::otel::LangfuseTracer;
use serde_json::{Value, json};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use wiremock::matchers::{method, path_regex};
use wiremock::{Mock, MockServer, Respond, ResponseTemplate};

use super::body_json;

/// The default Claude model of the SDK's examples, as a Bedrock inference profile.
pub const MODEL: &str = "us.anthropic.claude-sonnet-5-5";

/// Static test credentials: the SDK signs with them and looks up nothing.
pub fn credentials() -> Credentials {
    Credentials::new("AKIDTEST", "secret", None, None, "test")
}

/// SDK configuration for the fake Bedrock at `endpoint`: static credentials,
/// no retries.
pub fn sdk_config(endpoint: &str) -> aws_sdk_bedrockruntime::config::Builder {
    aws_sdk_bedrockruntime::Config::builder()
        .behavior_version(BehaviorVersion::latest())
        .region(Region::new("us-east-1"))
        .credentials_provider(credentials())
        .endpoint_url(endpoint)
        .retry_config(RetryConfig::disabled())
}

/// A traced client with `config`.
pub fn bedrock_with(
    tracer: &LangfuseTracer,
    config: aws_sdk_bedrockruntime::config::Builder,
) -> Bedrock {
    Bedrock::new(
        aws_sdk_bedrockruntime::Client::from_conf(config.build()),
        tracer,
    )
}

/// A traced client of the fake Bedrock `server`.
pub fn bedrock(tracer: &LangfuseTracer, server: &MockServer) -> Bedrock {
    bedrock_with(tracer, sdk_config(&server.uri()))
}

/// Answer every Converse call (`POST /model/{modelId}/converse`).
pub async fn mock_converse(server: &MockServer, response: impl Respond + 'static) {
    Mock::given(method("POST"))
        .and(path_regex("^/model/[^/]+/converse$"))
        .respond_with(response)
        .mount(server)
        .await;
}

/// Answer every ConverseStream call (`POST /model/{modelId}/converse-stream`).
pub async fn mock_converse_stream(server: &MockServer, response: impl Respond + 'static) {
    Mock::given(method("POST"))
        .and(path_regex("^/model/[^/]+/converse-stream$"))
        .respond_with(response)
        .mount(server)
        .await;
}

/// The requests the fake Bedrock received, as `(path, body)`.
pub async fn bedrock_requests(server: &MockServer) -> Vec<(String, Value)> {
    server
        .received_requests()
        .await
        .unwrap()
        .iter()
        .map(|request| (request.url.path().to_owned(), body_json(request)))
        .collect()
}

pub fn message(role: ConversationRole, content: Vec<ContentBlock>) -> Message {
    Message::builder()
        .role(role)
        .set_content(Some(content))
        .build()
        .unwrap()
}

/// A user message with one text block.
pub fn user(text: &str) -> Message {
    message(
        ConversationRole::User,
        vec![ContentBlock::Text(text.into())],
    )
}

/// A Smithy `Document` from JSON (tool inputs, schemas, model fields).
pub fn document(value: Value) -> Document {
    match value {
        Value::Null => Document::Null,
        Value::Bool(b) => Document::Bool(b),
        Value::Number(n) => Document::Number(match (n.as_u64(), n.as_i64()) {
            (Some(positive), _) => Number::PosInt(positive),
            (None, Some(negative)) => Number::NegInt(negative),
            (None, None) => Number::Float(n.as_f64().unwrap()),
        }),
        Value::String(s) => Document::String(s),
        Value::Array(items) => Document::Array(items.into_iter().map(document).collect()),
        Value::Object(map) => {
            Document::Object(map.into_iter().map(|(k, v)| (k, document(v))).collect())
        }
    }
}

/// Body of a Converse response whose message has `content`: 12 input and 3
/// output tokens next to 2000 cached ones, stop reason `end_turn`.
pub fn converse_body(content: Value) -> Value {
    json!({
        "output": {"message": {"role": "assistant", "content": content}},
        "stopReason": "end_turn",
        "usage": {
            "inputTokens": 12, "outputTokens": 3, "totalTokens": 2015,
            "cacheReadInputTokens": 2000, "cacheWriteInputTokens": 0
        },
        "metrics": {"latencyMs": 321}
    })
}

pub fn converse_response(content: Value) -> ResponseTemplate {
    super::json(200, converse_body(content))
}

/// An error response of the REST-JSON protocol: the error code is a header.
pub fn service_error(status: u16, code: &str, message: &str) -> ResponseTemplate {
    super::json(status, json!({"message": message})).insert_header("x-amzn-ErrorType", code)
}

/// One frame of the event stream of a ConverseStream response.
fn frame(
    message_type: &'static str,
    type_header: &'static str,
    name: &'static str,
    payload: Value,
) -> Vec<u8> {
    let header = |key: &'static str, value: &'static str| {
        Header::new(key, HeaderValue::String(value.into()))
    };
    let message = EventMessage::new(payload.to_string().into_bytes())
        .add_header(header(":message-type", message_type))
        .add_header(header(type_header, name))
        .add_header(header(":content-type", "application/json"));
    let mut frame = Vec::new();
    aws_smithy_eventstream::frame::write_message_to(&message, &mut frame).unwrap();
    frame
}

/// An event frame (`messageStart`, `contentBlockDelta`, ...).
pub fn event(event_type: &'static str, payload: Value) -> Vec<u8> {
    frame("event", ":event-type", event_type, payload)
}

/// An exception frame: a modeled error in the middle of the stream.
pub fn exception(exception_type: &'static str, message: &str) -> Vec<u8> {
    frame(
        "exception",
        ":exception-type",
        exception_type,
        json!({"message": message}),
    )
}

/// The frame of a text delta of content block `index`.
pub fn text_delta(index: u32, text: &str) -> Vec<u8> {
    event(
        "contentBlockDelta",
        json!({"contentBlockIndex": index, "delta": {"text": text}}),
    )
}

/// A ConverseStream response with all its frames in one body.
pub fn event_stream(frames: Vec<Vec<u8>>) -> ResponseTemplate {
    ResponseTemplate::new(200).set_body_raw(frames.concat(), "application/vnd.amazon.eventstream")
}

/// What a [`slow_bedrock`] does with its one response.
pub enum Step {
    /// Send an event-stream frame as one HTTP chunk.
    Frame(Vec<u8>),
    Pause(Duration),
    /// Cut the connection without ending the response.
    Hangup,
}

/// A fake Bedrock that answers its first request with a chunked event
/// stream sent step by step, so the client reads a stream that is still
/// open (wiremock only sends whole bodies). Returns the endpoint URL.
pub async fn slow_bedrock(steps: Vec<Step>) -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint = format!("http://{}", listener.local_addr().unwrap());
    tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        read_request(&mut socket).await;
        let head = "HTTP/1.1 200 OK\r\n\
                    content-type: application/vnd.amazon.eventstream\r\n\
                    transfer-encoding: chunked\r\n\r\n";
        socket.write_all(head.as_bytes()).await.unwrap();
        for step in steps {
            match step {
                Step::Pause(pause) => tokio::time::sleep(pause).await,
                Step::Hangup => return,
                Step::Frame(frame) => {
                    let mut chunk = format!("{:x}\r\n", frame.len()).into_bytes();
                    chunk.extend_from_slice(&frame);
                    chunk.extend_from_slice(b"\r\n");
                    // The client may have dropped the stream.
                    if socket.write_all(&chunk).await.is_err() {
                        return;
                    }
                }
            }
        }
        let _ = socket.write_all(b"0\r\n\r\n").await;
    });
    endpoint
}

/// Read one HTTP request with a `content-length` body.
async fn read_request(socket: &mut tokio::net::TcpStream) {
    let mut request = Vec::new();
    let mut buffer = [0_u8; 4096];
    loop {
        let read = socket.read(&mut buffer).await.unwrap();
        assert!(read > 0, "the connection closed before the request ended");
        request.extend_from_slice(&buffer[..read]);
        let Some(head_end) = request.windows(4).position(|w| w == b"\r\n\r\n") else {
            continue;
        };
        let head = String::from_utf8_lossy(&request[..head_end]).to_ascii_lowercase();
        let body_length: usize = head
            .lines()
            .find_map(|line| line.strip_prefix("content-length:"))
            .map_or(0, |value| value.trim().parse().unwrap());
        if request.len() >= head_end + 4 + body_length {
            return;
        }
    }
}
