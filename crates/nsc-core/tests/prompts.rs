use nsc_core::models::{
    Chapter, TransformationChapter, TransformationNovel, TransformMode, TransformStatus,
};
use nsc_core::prompts::{builtin_prompts, render, render_raw, PromptContext, PromptVars, REQUIRED_PLACEHOLDERS};

fn n() -> TransformationNovel {
    TransformationNovel {
        id: 1,
        data_asset_id: 1,
        title: "T".into(),
        created_at: chrono::Utc::now(),
        default_model_config_id: None,
        default_prompt_id: None,
        default_mode: None,
    }
}
fn ch(id: i64, idx: i32, body: &str) -> (Chapter, String) {
    let chapter = Chapter {
        id,
        data_asset_id: 1,
        idx,
        title: format!("ch{idx}"),
        byte_start: 0,
        byte_end: body.len() as i64,
        word_count: 1,
    };
    (chapter, body.to_string())
}

#[test]
fn substitutes_chapter_and_title() {
    let novel = n();
    let (chapter, body) = ch(1, 1, "正文ABC");
    let prev: Vec<(String, String)> = vec![];
    let next: Vec<(String, String)> = vec![];
    let tx: Vec<TransformationChapter> = vec![];
    let ctx = PromptContext {
        transformation_novel: &novel,
        chapter: &chapter,
        chapter_content: &body,
        prev_original: &prev,
        prev_transformed: &tx,
        next_original: &next,
    };
    let out = render("title={{chapter_title}} body={{chapter_content}}", &ctx).unwrap();
    assert_eq!(out, "title=ch1 body=正文ABC");
}

#[test]
fn missing_prev_transformed_renders_empty() {
    let novel = n();
    let (chapter, body) = ch(1, 1, "X");
    let prev_orig = vec![
        ("ch0".to_string(), "前文A".to_string()),
        ("ch0".to_string(), "前文B".to_string()),
    ];
    let tx: Vec<TransformationChapter> = vec![];
    let next: Vec<(String, String)> = vec![];
    let ctx = PromptContext {
        transformation_novel: &novel,
        chapter: &chapter,
        chapter_content: &body,
        prev_original: &prev_orig,
        prev_transformed: &tx,
        next_original: &next,
    };
    let tpl = "[prev_original]\n{{prev_original}}\n[prev_transformed]\n{{prev_transformed}}\n[end]";
    let out = render(tpl, &ctx).unwrap();
    assert!(out.contains("前文A"));
    assert!(out.contains("前文B"));
    // 无已转换参考时:不输出"(暂无已转换参考)"占位符,prev_transformed 段保持空。
    // 这样 LLM 看到的 prompt 在"有没有参考"上行为一致,不被模板提示词误导。
    assert!(!out.contains("(暂无已转换参考)"));
    assert!(out.contains("[prev_transformed]\n\n[end]"));
}

#[test]
fn existing_prev_transformed_is_concatenated_in_order() {
    let novel = n();
    let (chapter, body) = ch(1, 1, "X");
    let prev_orig: Vec<(String, String)> = vec![];
    let next: Vec<(String, String)> = vec![];
    let tx = vec![
        TransformationChapter {
            id: 11,
            transformation_novel_id: 1,
            chapter_id: 99,
            mode: TransformMode::Style,
            prompt_id: 1,
            model_config_id: 1,
            ctx_prev_original: 0,
            ctx_prev_transformed: 0,
            ctx_next_original: 0,
            status: TransformStatus::Done,
            result_content: Some("REF1".into()),
            tokens_in: None,
            tokens_out: None,
            error: None,
            started_at: None,
            completed_at: None,
            batch_id: None,
            style_ref_chapter_id: None,
        },
        TransformationChapter {
            id: 22,
            transformation_novel_id: 1,
            chapter_id: 100,
            mode: TransformMode::Style,
            prompt_id: 1,
            model_config_id: 1,
            ctx_prev_original: 0,
            ctx_prev_transformed: 0,
            ctx_next_original: 0,
            status: TransformStatus::Done,
            result_content: Some("REF2".into()),
            tokens_in: None,
            tokens_out: None,
            error: None,
            started_at: None,
            completed_at: None,
            batch_id: None,
            style_ref_chapter_id: None,
        },
    ];
    let ctx = PromptContext {
        transformation_novel: &novel,
        chapter: &chapter,
        chapter_content: &body,
        prev_original: &prev_orig,
        prev_transformed: &tx,
        next_original: &next,
    };
    let out = render("{{prev_transformed}}", &ctx).unwrap();
    assert!(out.find("REF1").unwrap() < out.find("REF2").unwrap());
}

#[test]
fn raw_replaces_all_seven_vars() {
    // 新 schema 下 TransformationNovel 没有 author 字段,
    // render() 内部把 author 写死为空字符串 —— 这里走 render_raw 直接给 vars,
    // 显式传 author = "A" 让 |A 断言仍能覆盖到「所有 7 个 var 都替换」的原意。
    let vars = PromptVars {
        chapter_title: "T1".into(),
        chapter_content: "BODY".into(),
        prev_original: "PREV".into(),
        next_original: "NEXT".into(),
        prev_transformed: "REF".into(),
        novel_title: "N".into(),
        author: "A".into(),
    };
    let tpl = "{{chapter_title}}|{{chapter_content}}|{{prev_original}}|{{next_original}}|{{prev_transformed}}|{{novel_title}}|{{author}}";
    assert_eq!(render_raw(tpl, &vars), "T1|BODY|PREV|NEXT|REF|N|A");
}

#[test]
fn raw_leaves_unknown_vars_untouched() {
    let vars = PromptVars::default();
    assert_eq!(
        render_raw("hello {{not_a_real_var}} world", &vars),
        "hello {{not_a_real_var}} world"
    );
}

#[test]
fn raw_passes_through_when_no_vars() {
    let vars = PromptVars::default();
    assert_eq!(render_raw("plain text", &vars), "plain text");
}

#[test]
fn raw_repeated_var_replaced_each_time() {
    let vars = PromptVars {
        chapter_title: "X".into(),
        ..PromptVars::default()
    };
    assert_eq!(
        render_raw(
            "{{chapter_title}}|{{chapter_title}}|{{chapter_title}}",
            &vars
        ),
        "X|X|X"
    );
}

#[test]
fn raw_empty_template_returns_empty() {
    let vars = PromptVars::default();
    assert_eq!(render_raw("", &vars), "");
}

/// builtin 模板必须含 `{{chapter_title}}` 和 `{{chapter_content}}`,否则
/// LLM 看不到章节正文 —— 历史 bug 是 builtin 用了单花括号 `{chapter_content}`,
/// render_raw 只替换双花括号,LLM 收到字面量追问"请提供章节内容..."。
/// 锁住这条契约,避免 builtin 再误写成单花括号。
#[test]
fn builtin_templates_reference_chapter_content() {
    for bp in builtin_prompts() {
        for needle in REQUIRED_PLACEHOLDERS {
            assert!(
                bp.template.contains(needle),
                "builtin prompt `{}` missing required placeholder `{}`",
                bp.name,
                needle,
            );
        }
    }
}