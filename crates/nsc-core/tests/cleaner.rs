use nsc_core::cleaner::{apply_rules, default_rules, RuleId};

#[test]
fn add_indent_inserts_two_full_width_spaces_before_unindented_non_blank_lines() {
    let input = "第一行\n第二行\n";
    let rules = vec![RuleId::AddIndentToUnindented];
    let out = apply_rules(input, &rules);
    // 首行不算(注释里写"首行是标题"语义)。这里规则单独跑 = 给所有非空未缩进行加。
    assert_eq!(out, "　　第一行\n　　第二行\n");
}

#[test]
fn add_indent_skips_blank_lines_and_existing_indent() {
    let input = "　　已缩进\n\n裸行\n";
    let rules = vec![RuleId::AddIndentToUnindented];
    let out = apply_rules(input, &rules);
    assert_eq!(out, "　　已缩进\n\n　　裸行\n");
}

#[test]
fn merge_short_joins_wrapped_lines_that_dont_end_with_punctuation() {
    let input = "今天天气很\n不错,我们去\n公园散步。\n";
    let rules = vec![RuleId::MergeShortParagraphs];
    let out = apply_rules(input, &rules);
    assert_eq!(out, "今天天气很不错,我们去公园散步。\n");
}

#[test]
fn merge_short_preserves_blank_lines_as_paragraph_separators() {
    let input = "第一段\n接着说\n\n新的一段\n";
    let rules = vec![RuleId::MergeShortParagraphs];
    let out = apply_rules(input, &rules);
    assert_eq!(out, "第一段接着说\n\n新的一段\n");
}

#[test]
fn merge_short_does_not_join_lines_ending_with_punctuation() {
    let input = "上一句说完了。\n下一句开头\n";
    let rules = vec![RuleId::MergeShortParagraphs];
    let out = apply_rules(input, &rules);
    assert_eq!(out, "上一句说完了。\n下一句开头\n");
}

#[test]
fn merge_short_joins_lines_ending_with_chinese_comma() {
    // 行尾中文逗号 = 分句未完成,强制合并下一行,覆盖"行尾有标点不合并"语义。
    let input = "今天天气很，\n不错。\n";
    let rules = vec![RuleId::MergeShortParagraphs];
    let out = apply_rules(input, &rules);
    assert_eq!(out, "今天天气很，不错。\n");
}

#[test]
fn merge_short_joins_lines_ending_with_english_comma() {
    // 英文逗号同语义。
    let input = "today is sunny,\nluckily.\n";
    let rules = vec![RuleId::MergeShortParagraphs];
    let out = apply_rules(input, &rules);
    assert_eq!(out, "today is sunny,luckily.\n");
}

#[test]
fn collapse_blank_runs_collapses_three_or_more_blank_lines_to_two() {
    let input = "A\n\n\n\nB\n";
    let rules = vec![RuleId::CollapseBlankRuns];
    let out = apply_rules(input, &rules);
    assert_eq!(out, "A\n\nB\n");
}

#[test]
fn collapse_blank_runs_preserves_single_blank_line_as_paragraph_separator() {
    let input = "A\n\nB\n";
    let rules = vec![RuleId::CollapseBlankRuns];
    let out = apply_rules(input, &rules);
    assert_eq!(out, "A\n\nB\n");
}

#[test]
fn rule_order_matters_add_indent_before_merge_short() {
    let input = "今天\n天气\n";
    let r1 = apply_rules(input, &[RuleId::AddIndentToUnindented, RuleId::MergeShortParagraphs]);
    let r2 = apply_rules(input, &[RuleId::MergeShortParagraphs, RuleId::AddIndentToUnindented]);
    assert_ne!(r1, r2);
}

#[test]
fn default_rules_idempotent() {
    let input = "今天天气很\n不错。\n\n\n\n明天也\n一样好。\n";
    let once = apply_rules(input, &default_rules());
    let twice = apply_rules(&once, &default_rules());
    assert_eq!(once, twice, "default rule set must converge after one pass");
}

#[test]
fn default_rules_merge_short_runs_before_add_indent() {
    // 回归测试:默认顺序必须先合并再加缩进,否则合并永远是 no-op(已缩进的行
    // 都以 　　开头,merge 的 starts_with(INDENT) 守卫会把它们全跳过)。
    let input = "今天\n天气\n";
    let out = apply_rules(input, &default_rules());
    assert_eq!(out, "　　今天天气\n");
}

#[test]
fn apply_rules_normalizes_crlf_line_endings() {
    // 用户 .txt 多半是 Windows 行结尾,apply_rules 入口要把 \r\n / 孤立 \r
    // 都换成 \n。否则 merge 把 4 行逻辑拼成 1 行后,中间残留的 \r 在
    // 浏览器 <textarea> 里仍被渲染成换行 → 视觉上跟原文一样,用户看着"合并无效"。
    let input = "他微笑道:「你既然现在有些事情,我自然也不强留你。这样吧,我们\r\n便做个游戏。明年七月初七,我们在京城中互相寻找,谁也不能赖皮。若是我先\r\n寻到你,我便亲你一百下,你若先寻到我,我就吃点亏,让你亲我一百下。但是\r\n谁要敢赖皮,我就打她的小屁股一百下。」";
    let out = apply_rules(input, &[RuleId::MergeShortParagraphs]);
    assert_eq!(out.lines().count(), 1, "4 行 CRLF 输入应该合并成 1 行");
    assert!(!out.contains('\r'), "输出里不能留 \\r,否则 textarea 会把它当换行");
    assert!(!out.contains('\n'), "4 行合并后中间不应该再有 \\n");
    assert!(out.starts_with("他微笑道"));
    assert!(out.ends_with("打她的小屁股一百下。」"));
    assert!(out.contains("但是谁要敢赖皮"));
}

#[test]
fn apply_rules_strips_lone_cr() {
    // 老 Mac 行结尾 / 复制粘贴残留可能留下孤立 \r,同样需要归一化。
    let input = "第一段\r继续说\r\r新段";
    let out = apply_rules(input, &[RuleId::MergeShortParagraphs]);
    assert_eq!(out, "第一段继续说\n\n新段");
}

#[test]
fn ensure_blank_inserts_blank_line_between_adjacent_paragraphs() {
    // 两条相邻非空行 → 中间插一个空行;前/后是空行的不重复插(避免连空行)。
    let input = "段1\n段2\n段3";
    let rules = vec![RuleId::EnsureBlankLineBetweenParagraphs];
    let out = apply_rules(input, &rules);
    assert_eq!(out, "段1\n\n段2\n\n段3");
}

#[test]
fn ensure_blank_does_not_double_up_existing_blank() {
    // 用户输入已经有空行分隔 → 不要变成 2 个空行。
    let input = "段1\n\n段2";
    let rules = vec![RuleId::EnsureBlankLineBetweenParagraphs];
    let out = apply_rules(input, &rules);
    assert_eq!(out, "段1\n\n段2");
}

#[test]
fn ensure_blank_after_merge_then_indent_preserves_blank_lines() {
    // 完整链路:merge → ensure_blank → add_indent → collapse_blank
    // 输入两段各 2 行折行,期望输出:缩进 + 段1 / 空行 / 缩进 + 段2
    let input = "段1第一行\n段1第二行。\n段2第一行\n段2第二行。\n";
    let out = apply_rules(input, &default_rules());
    assert_eq!(out, "　　段1第一行段1第二行。\n\n　　段2第一行段2第二行。\n");
}