pub mod builtin;
pub mod render;

pub use builtin::{builtin_prompts, BuiltinPrompt};
pub use render::{render, render_raw, PromptContext, PromptVars};

/// builtin 模板必须包含的最低 placeholder 集。回归测试
/// `prompts::builtin_templates_reference_chapter_content` 锁住这条契约。
pub const REQUIRED_PLACEHOLDERS: &[&str] = &["{{chapter_title}}", "{{chapter_content}}"];
