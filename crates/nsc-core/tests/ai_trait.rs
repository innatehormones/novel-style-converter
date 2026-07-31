use async_trait::async_trait;
use nsc_core::ai::{AiProvider, ChatMessage, ChatRequest, ChatResponse, Role};

struct Echo;
#[async_trait]
impl AiProvider for Echo {
    async fn chat(&self, req: ChatRequest) -> nsc_core::Result<ChatResponse> {
        let last = req.messages.last().unwrap();
        Ok(ChatResponse {
            content: format!("echo:{}", last.content),
            tokens_in: last.content.len() as i32,
            tokens_out: 1,
        })
    }
}

#[tokio::test]
async fn echo_returns_dto() {
    let p = Echo;
    let req = ChatRequest {
        model: "x".into(),
        messages: vec![ChatMessage { role: Role::User, content: "hi".into() }],
        temperature: None, max_tokens: None,
    };
    let r = p.chat(req).await.unwrap();
    assert_eq!(r.content, "echo:hi");
}