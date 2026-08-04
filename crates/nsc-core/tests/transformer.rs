use async_trait::async_trait;
use nsc_core::ai::{AiProvider, ChatRequest, ChatResponse};
use nsc_core::models::{Chapter, ModelConfig, Prompt, PromptKind, TransformationNovel};
use nsc_core::transformer::{
    DefaultTransformer, TransformationNovelContext, TransformRequest, Transformer,
};

struct Fixed(String);
#[async_trait]
impl AiProvider for Fixed {
    async fn chat(&self, req: ChatRequest) -> nsc_core::Result<ChatResponse> {
        let joined = req.messages.iter()
            .map(|m| m.content.as_str()).collect::<Vec<_>>().join("|");
        assert!(joined.contains("正文ABC"));
        Ok(ChatResponse { content: format!("OUT:{}", self.0), tokens_in: 5, tokens_out: 7 })
    }
}

fn fixture() -> (TransformationNovel, Chapter, Prompt, ModelConfig) {
    let novel = TransformationNovel {
        id: 1, data_asset_id: 1, title: "T".into(),
        created_at: chrono::Utc::now(),
        default_model_config_id: None,
        default_prompt_id: None,
        default_mode: None,
    };
    let chapter = Chapter {
        id: 2, data_asset_id: 1, idx: 5, title: "ch5".into(),
        byte_start: 0, byte_end: 7, word_count: 1,
    };
    let prompt = Prompt {
        id: 1, name: "p".into(), kind: PromptKind::Compress,
        template: "T=[{{chapter_title}}] {{chapter_content}}".into(), is_builtin: false,
    };
    let model = ModelConfig {
        id: 1, name: "m".into(), base_url: "x".into(),
        api_key: "k".into(), model: "x".into(),
        max_tokens: None, temperature: None, concurrency: 1,
    };
    (novel, chapter, prompt, model)
}

#[tokio::test]
async fn renders_prompt_and_calls_provider() {
    let (novel, chapter, prompt, model) = fixture();
    let ai = Fixed("DONE".into());
    let t = DefaultTransformer { ai: Box::new(ai) };
    let req = TransformRequest {
        chapter,
        chapter_content: "正文ABC".into(),
        novel_context: TransformationNovelContext {
            transformation_novel: novel,
            prev_original: vec![],
            prev_transformed: vec![],
            next_original: vec![],
        },
        prompt,
        model_config: model,
    };
    let out = t.transform(req).await.unwrap();
    assert_eq!(out.result_content, "OUT:DONE");
    assert_eq!(out.tokens_in, 5);
    assert_eq!(out.tokens_out, 7);
}