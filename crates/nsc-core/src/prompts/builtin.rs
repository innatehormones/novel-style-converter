use crate::models::PromptKind;

#[derive(Debug, Clone)]
pub struct BuiltinPrompt {
    pub name: &'static str,
    pub kind: PromptKind,
    pub template: &'static str,
}

pub fn builtin_prompts() -> Vec<BuiltinPrompt> {
    vec![
        BuiltinPrompt {
            name: "compress_default",
            kind: PromptKind::Compress,
            template: "你是一名专业的小说编辑，请对以下章节进行压缩（保留关键情节和人物，去除冗余描写）。\n目标压缩比例为原文的 60%~70%。\n\n# 上一章原文\n{prev_original}\n\n# 上一章改写后\n{prev_transformed}\n\n# 下一章原文\n{next_original}\n\n# 当前章节\n{chapter_title}\n{chapter_content}\n\n请只输出压缩后的章节正文，不要附加说明。",
        },
        BuiltinPrompt {
            name: "style_default",
            kind: PromptKind::Style,
            template: "你是一名资深网文风格改写编辑，请将以下章节改写为更紧凑、更有张力的现代网文风格，保持人物性格和情节走向不变。\n\n# 上下文（前几章已改写）\n{prev_transformed}\n\n# 当前章节\n{chapter_title}\n{chapter_content}\n\n请只输出改写后的章节正文，不要附加说明。",
        },
    ]
}
