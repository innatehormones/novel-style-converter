use nsc_core::splitter::{ChapterSplitter, DefaultSplitter};

#[test]
fn splits_chinese_chapter_titles() {
    let text = "\
第一章 山村少年
天亮了，小明起床。
第二章 走出门
他去上山砍柴。

第三章 又一天
打怪升级。";
    let r = DefaultSplitter.split(text);
    assert_eq!(r.chapters.len(), 3);
    assert_eq!(r.chapters[0].title, "第一章 山村少年");
    assert!(r.chapters[1].content.contains("上山砍柴"));
}

#[test]
fn splits_by_blank_lines_when_no_titles() {
    let text = "段落一内容。\n\n段落二内容。\n\n段落三内容。";
    let r = DefaultSplitter.split(text);
    assert_eq!(r.chapters.len(), 3);
}

#[test]
fn word_count_is_chinese_aware() {
    let text = "第一章 aaa\nhello 你好 世界";
    let r = DefaultSplitter.split(text);
    assert!(r.chapters[0].word_count >= 4);
}

#[test]
fn empty_input_yields_no_chapter() {
    let r = DefaultSplitter.split("");
    assert_eq!(r.chapters.len(), 0);
}

#[test]
fn byte_range_slices_match_chapter_boundaries() {
    // 用户报告 "DataAsset.vue 章节内容对不上" 时怀疑 splitter 的 byte_range 算错。
    // 验证:每个 chapter 的 [byte_start, byte_end) 在原文里切片,必须包含 title 且
    // 切片邻接(本章 byte_end == 下一章 byte_start)。后者排除"累进错位"的常见 bug。
    let text = "\
　　第一章 标题一
第一段第一句。
第一段第二句。

　　第二章 标题二
第二段第一句。
第二段第二句。

　　第三章 标题三
第三段第一句。
第三段第二句。

　　第四章 标题四
第四段第一句。
";
    let r = DefaultSplitter.split(text);
    assert!(r.chapters.len() >= 4, "splitter 应切出 ≥4 章");
    for ch in &r.chapters {
        let slice = &text[ch.byte_start..ch.byte_end];
        assert!(
            slice.contains(&ch.title),
            "byte_range 切片应包含 chapter.title: title={:?} slice={:?}",
            ch.title, slice
        );
    }
    for w in r.chapters.windows(2) {
        assert_eq!(
            w[0].byte_end, w[1].byte_start,
            "相邻章节 byte 范围必须邻接(本章 byte_end == 下一章 byte_start),否则前端切片会重叠或漏"
        );
    }
    assert_eq!(
        r.chapters.last().unwrap().byte_end,
        text.len(),
        "最后一章 byte_end 必须等于 text.len()"
    );
}

#[test]
fn head_before_first_chapter_is_dropped() {
    // 元数据(书名/作者/简介)放在第一个「第N章」之前 → 视为 head 丢弃,
    // 不作为首章。期望 chapters 只含 2 个正文章节。
    let text = "\
书名:凡人修仙传
作者:忘语
简介:一个山村少年的修仙之路。

第一章 山村少年
天亮了,小明起床。

第二章 走出门
他去上山砍柴。";
    let r = DefaultSplitter.split(text);
    assert_eq!(r.chapters.len(), 2);
    assert_eq!(r.chapters[0].title, "第一章 山村少年");
    assert_eq!(r.chapters[1].title, "第二章 走出门");
}

#[test]
fn splits_fullwidth_paren_chapter_headings() {
    // 一些小说用「书名:章节名(全角括号 X)」作为章节标题(整行收尾)。
    // 整行只有水平空白 + 短前缀 + (X) 才是标题;正文里出现 "(X)" 不算。
    let text = "\
书名:凡人修仙传
作者:忘语

　　凡人修仙传：云游篇（一）
　　正文一……

　　凡人修仙传：云游篇（二）
　　正文二……

　　凡人修仙传：云游篇（三）
　　正文三……";
    let r = DefaultSplitter.split(text);
    assert_eq!(r.chapters.len(), 3);
    assert!(r.chapters[0].title.contains("（一）"));
    assert!(r.chapters[1].title.contains("（二）"));
    assert!(r.chapters[2].title.contains("（三）"));
}

#[test]
fn fullwidth_paren_in_prose_is_not_a_chapter() {
    // 正文里出现 "(一)" 不是章节标记;只有整行收尾在 (X) 才算标题。
    let text = "\
他说「我(一)个人去。」

她说「那(二)件事情先放放。」

他说「好(三)吧。」
";
    let r = DefaultSplitter.split(text);
    // 没匹配到 chapter regex → 走空行 fallback → 按 \n\n 切成多段。
    // 关键断言:章标题里出现的 "(X)" 不会被作为标题收尾,
    // 即不能等于「(一)」之类,必须仍是带正文上下文的首行。
    assert!(r.chapters.len() >= 2, "应当按空行 fallback 切成多段");
    for c in &r.chapters {
        assert!(
            c.title.contains("。") || c.title.contains("「"),
            "章标题应保留正文上下文,而不是被识别成 (X) 标题: {:?}",
            c.title
        );
    }
}

#[test]
fn splits_crlf_text_with_paren_chapters() {
    // CRLF + (X) 风格:确认 \r\r\n 段落分隔在 chapter path 下不影响识别。
    let text = "邀请您访问搜书吧\r\r\nhttps://example.com/?fromuid=1\r\r\n\
　　凡人修仙传：云游篇（一）\r\r\n\
正文一\r\r\n\
　　凡人修仙传：云游篇（二）\r\r\n\
正文二";
    let r = DefaultSplitter.split(text);
    // head(邀请您访问…https…)应被丢弃,正文章节 2 个
    assert_eq!(r.chapters.len(), 2);
    assert!(r.chapters[0].title.contains("（一）"));
    assert!(r.chapters[1].title.contains("（二）"));
}