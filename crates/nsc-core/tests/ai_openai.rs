use nsc_core::ai::{AiProvider, ChatMessage, ChatRequest, OpenAiProvider, Role};
use serde_json::json;
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn chat_req() -> ChatRequest {
    ChatRequest {
        model: "x-model".into(),
        messages: vec![ChatMessage { role: Role::User, content: "hello".into() }],
        temperature: Some(0.5), max_tokens: Some(64),
    }
}

#[tokio::test]
async fn success_extracts_content_and_usage() {
    let server = MockServer::start().await;
    Mock::given(method("POST")).and(path("/chat/completions"))
        .and(header("authorization", "Bearer sk-test"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": "x", "object": "chat.completion",
            "choices": [{
                "index": 0,
                "message": { "role": "assistant", "content": "你好" },
                "finish_reason": "stop"
            }],
            "usage": { "prompt_tokens": 10, "completion_tokens": 3, "total_tokens": 13 }
        }))).mount(&server).await;

    let p = OpenAiProvider::new(server.uri(), "sk-test".into()).unwrap();
    let r = p.chat(chat_req()).await.unwrap();
    assert_eq!(r.content, "你好");
    assert_eq!(r.tokens_in, 10);
    assert_eq!(r.tokens_out, 3);
}

#[tokio::test]
async fn unauthorized_becomes_ai_error() {
    let server = MockServer::start().await;
    Mock::given(method("POST")).and(path("/chat/completions"))
        .respond_with(ResponseTemplate::new(401).set_body_string("unauthorized"))
        .mount(&server).await;
    let p = OpenAiProvider::new(server.uri(), "sk-bad".into()).unwrap();
    let r = p.chat(chat_req()).await;
    assert!(matches!(r, Err(nsc_core::Error::Ai(_))));
}

#[tokio::test]
async fn rate_limit_becomes_ai_error() {
    let server = MockServer::start().await;
    Mock::given(method("POST")).and(path("/chat/completions"))
        .respond_with(ResponseTemplate::new(429))
        .mount(&server).await;
    let p = OpenAiProvider::new(server.uri(), "sk".into()).unwrap();
    let r = p.chat(chat_req()).await;
    assert!(matches!(r, Err(nsc_core::Error::Ai(_))));
}