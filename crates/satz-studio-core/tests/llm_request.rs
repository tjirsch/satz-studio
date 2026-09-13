//! `body()` and `betas()`: the wire body of a request against a snapshot, the
//! breakpoint layout, and the betas per credential.

use satz_studio_core::llm::claude::types::{BETA_FALLBACK, BETA_OAUTH, betas, body};
use satz_studio_core::llm::{
    CacheControl, ContentBlock, Credential, Effort, Message, Request, Role, SystemBlock, ToolDef,
};

fn tool(name: &str, description: &str, schema: serde_json::Value, marked: bool) -> ToolDef {
    ToolDef {
        name: name.to_string(),
        description: description.to_string(),
        input_schema: schema,
        cache_control: marked.then(CacheControl::ephemeral),
    }
}

fn text(text: &str, marked: bool) -> ContentBlock {
    ContentBlock::Text {
        text: text.to_string(),
        cache_control: marked.then(CacheControl::ephemeral),
    }
}

/// Two tools in the wrong order with the marker on the wrong one, two system blocks
/// with the marker on the wrong one, a marker on an earlier user block: `body()` owns
/// the layout.
fn request() -> Request {
    Request {
        model: "claude-opus-5".to_string(),
        max_tokens: 64_000,
        system: vec![
            SystemBlock {
                text: "the guide".to_string(),
                cache_control: None,
            },
            SystemBlock {
                text: "estate: /estates/acme".to_string(),
                cache_control: Some(CacheControl::ephemeral()),
            },
        ],
        messages: vec![
            Message {
                role: Role::User,
                content: vec![text("hello", true)],
            },
            Message {
                role: Role::Assistant,
                content: vec![
                    ContentBlock::Thinking {
                        thinking: "a short thought".to_string(),
                        signature: "EqQBCgIYAhIM".to_string(),
                    },
                    text("hi", false),
                ],
            },
            Message {
                role: Role::User,
                content: vec![text("and", false), text("again", false)],
            },
        ],
        tools: vec![
            tool(
                "satz_transpile_check",
                "Check an estate.",
                serde_json::json!({"type": "object", "properties": {"estate": {"type": "string"}}}),
                true,
            ),
            tool(
                "satz_adopt",
                "Resolve live ids.",
                serde_json::json!({"type": "object", "properties": {}}),
                false,
            ),
        ],
        effort: Effort::Xhigh,
        fallbacks: true,
    }
}

#[test]
fn the_body_matches_the_snapshot() {
    let expected: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(format!(
            "{}/tests/fixtures/llm/request_body.json",
            env!("CARGO_MANIFEST_DIR")
        ))
        .expect("the snapshot exists"),
    )
    .expect("JSON");
    assert_eq!(body(&request()), expected);
}

#[test]
fn the_breakpoints_sit_on_the_last_tool_system_zero_and_the_last_user_block() {
    let b = body(&request());
    let tools = b["tools"].as_array().expect("tools");
    assert_eq!(
        tools
            .iter()
            .map(|t| t["name"].as_str().unwrap())
            .collect::<Vec<_>>(),
        vec!["satz_adopt", "satz_transpile_check"]
    );
    assert!(tools[0].get("cache_control").is_none());
    assert_eq!(
        tools[1]["cache_control"],
        serde_json::json!({"type": "ephemeral"})
    );
    assert_eq!(
        b["system"][0]["cache_control"],
        serde_json::json!({"type": "ephemeral"})
    );
    assert!(b["system"][1].get("cache_control").is_none());
    assert!(
        b["messages"][0]["content"][0]
            .get("cache_control")
            .is_none()
    );
    assert!(
        b["messages"][2]["content"][0]
            .get("cache_control")
            .is_none()
    );
    assert_eq!(
        b["messages"][2]["content"][1]["cache_control"],
        serde_json::json!({"type": "ephemeral"})
    );
    let markers = b.to_string().matches("\"ephemeral\"").count();
    assert_eq!(markers, 3);
}

#[test]
fn thinking_is_adaptive_and_summarised_effort_is_in_output_config_and_nothing_names_a_budget() {
    let b = body(&request());
    assert_eq!(
        b["thinking"],
        serde_json::json!({"type": "adaptive", "display": "summarized"})
    );
    assert_eq!(b["output_config"], serde_json::json!({"effort": "xhigh"}));
    assert_eq!(b["stream"], serde_json::json!(true));
    assert_eq!(b["max_tokens"], serde_json::json!(64_000));
    assert!(!b.to_string().contains("budget_tokens"));
    assert!(!b.to_string().contains("disabled"));
}

#[test]
fn fallbacks_are_sent_only_when_on() {
    assert_eq!(body(&request())["fallbacks"], serde_json::json!("default"));
    let mut off = request();
    off.fallbacks = false;
    assert!(body(&off).get("fallbacks").is_none());
}

#[test]
fn empty_tools_and_system_are_omitted() {
    let mut req = request();
    req.tools.clear();
    req.system.clear();
    let b = body(&req);
    assert!(b.get("tools").is_none());
    assert!(b.get("system").is_none());
}

#[test]
fn a_tool_result_as_the_last_user_block_carries_the_breakpoint() {
    let mut req = request();
    req.messages.push(Message {
        role: Role::Assistant,
        content: vec![ContentBlock::ToolUse {
            id: "toolu_01".to_string(),
            name: "satz_adopt".to_string(),
            input: serde_json::json!({}),
        }],
    });
    req.messages.push(Message {
        role: Role::User,
        content: vec![ContentBlock::ToolResult {
            tool_use_id: "toolu_01".to_string(),
            content: "{}".to_string(),
            is_error: false,
            cache_control: None,
        }],
    });
    let b = body(&req);
    assert_eq!(
        b["messages"][4]["content"][0]["cache_control"],
        serde_json::json!({"type": "ephemeral"})
    );
    assert!(
        b["messages"][4]["content"][0].get("is_error").is_none(),
        "a false is_error is not sent"
    );
    assert!(
        b["messages"][2]["content"][1]
            .get("cache_control")
            .is_none(),
        "the earlier user block lost its marker"
    );
}

#[test]
fn betas_follow_the_credential_and_the_fallback_setting() {
    let on = request();
    let mut off = request();
    off.fallbacks = false;
    let key = Credential::ApiKey("sk-example".to_string());
    let bearer = Credential::Bearer("tok-example".to_string());
    assert_eq!(betas(&on, &key), vec![BETA_FALLBACK]);
    assert_eq!(betas(&on, &bearer), vec![BETA_OAUTH, BETA_FALLBACK]);
    assert_eq!(betas(&off, &bearer), vec![BETA_OAUTH]);
    assert!(betas(&off, &key).is_empty());
}

#[test]
fn every_block_kind_round_trips_through_json() {
    let blocks = vec![
        text("t", true),
        ContentBlock::Thinking {
            thinking: "th".to_string(),
            signature: "sig".to_string(),
        },
        ContentBlock::RedactedThinking {
            data: "opaque".to_string(),
        },
        ContentBlock::ToolUse {
            id: "toolu_01".to_string(),
            name: "satz_adopt".to_string(),
            input: serde_json::json!({"a": [1, 2]}),
        },
        ContentBlock::ToolResult {
            tool_use_id: "toolu_01".to_string(),
            content: "done".to_string(),
            is_error: true,
            cache_control: None,
        },
        ContentBlock::Other(
            serde_json::json!({"type": "image", "source": {"type": "url", "url": "https://example.com/a.png"}}),
        ),
    ];
    let json = serde_json::to_string(&Message {
        role: Role::User,
        content: blocks.clone(),
    })
    .expect("serialises");
    let back: Message = serde_json::from_str(&json).expect("deserialises");
    assert_eq!(back.content, blocks);
    assert!(json.contains("\"type\":\"redacted_thinking\""));
    assert!(json.contains("\"is_error\":true"));
    assert!(
        matches!(serde_json::from_str::<ContentBlock>(r#"{"text": "no type"}"#), Err(e) if e.to_string().contains("`type`"))
    );
}

#[test]
fn the_credential_debug_form_hides_the_secret() {
    assert!(!format!("{:?}", Credential::ApiKey("sk-very-secret".to_string())).contains("secret"));
    assert!(!format!("{:?}", Credential::Bearer("tok-very-secret".to_string())).contains("secret"));
}
