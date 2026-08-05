use crate::error::Result;
use crate::models::{Chapter, TransformationChapter, TransformationNovel};

pub struct PromptContext<'a> {
    pub transformation_novel: &'a TransformationNovel,
    pub chapter: &'a Chapter,
    /// 章节正文切片(已从 uploads.original_text[byte_start..byte_end] 取出)。
    pub chapter_content: &'a str,
    /// 邻章上下文片段:(title, content) 对 —— 由 queue.rs 切片后传入。
    pub prev_original: &'a [(String, String)],
    pub prev_transformed: &'a [TransformationChapter],
    pub next_original: &'a [(String, String)],
}

fn join_chapter_pairs(parts: &[(String, String)], sep: &str, header: &str) -> String {
    if parts.is_empty() {
        return String::new();
    }
    let body = parts
        .iter()
        .map(|(title, content)| format!("{title}\n{content}"))
        .collect::<Vec<_>>()
        .join(sep);
    format!("{header}\n{body}")
}

fn join_transformations(parts: &[TransformationChapter], sep: &str, header: &str) -> String {
    let done: Vec<&TransformationChapter> = parts
        .iter()
        .filter(|t| t.result_content.is_some())
        .collect();
    if done.is_empty() {
        return String::new();
    }
    let body = done
        .into_iter()
        .filter_map(|t| t.result_content.clone())
        .collect::<Vec<_>>()
        .join(sep);
    format!("{header}\n{body}")
}

#[derive(Debug, Clone, Default)]
pub struct PromptVars {
    pub chapter_title: String,
    pub chapter_content: String,
    pub prev_original: String,
    pub next_original: String,
    pub prev_transformed: String,
    pub novel_title: String,
    pub author: String,
}

pub fn render_raw(template: &str, vars: &PromptVars) -> String {
    let mut out = template.to_string();
    out = out.replace("{{chapter_title}}", &vars.chapter_title);
    out = out.replace("{{chapter_content}}", &vars.chapter_content);
    out = out.replace("{{prev_original}}", &vars.prev_original);
    out = out.replace("{{next_original}}", &vars.next_original);
    out = out.replace("{{prev_transformed}}", &vars.prev_transformed);
    out = out.replace("{{novel_title}}", &vars.novel_title);
    out = out.replace("{{author}}", &vars.author);
    out
}

pub fn render(template: &str, ctx: &PromptContext<'_>) -> Result<String> {
    // 新设计下没有 author 字段;transformation_novel.title 充当 {{novel_title}}。
    let vars = PromptVars {
        chapter_title: ctx.chapter.title.clone(),
        chapter_content: ctx.chapter_content.to_string(),
        prev_original: join_chapter_pairs(ctx.prev_original, "\n\n", ""),
        next_original: join_chapter_pairs(ctx.next_original, "\n\n", ""),
        prev_transformed: join_transformations(ctx.prev_transformed, "\n\n", ""),
        novel_title: ctx.transformation_novel.title.clone(),
        author: String::new(),
    };
    Ok(render_raw(template, &vars))
}