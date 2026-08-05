use once_cell::sync::Lazy;
use regex::Regex;

use crate::text::word_count;

#[derive(Debug, Clone)]
pub struct ParsedChapter {
    pub title: String,
    pub content: String,
    pub word_count: i32,
    /// 章节起点(含标题行)在原文的 byte offset。
    pub byte_start: usize,
    /// 章节终点(不含)在原文的 byte offset。
    pub byte_end: usize,
}

#[derive(Debug, Clone)]
pub struct SplitResult {
    pub chapters: Vec<ParsedChapter>,
}

pub trait ChapterSplitter: Send + Sync {
    /// 纯 regex 自动切。
    fn split(&self, text: &str) -> SplitResult;

    /// 用 marker (byte offset) 重切:在每个 marker 位置强切;段内继续 regex 自动细切。
    /// markers 必须升序,且 < text.len();空 markers 退化为 split。
    fn split_with_markers(&self, text: &str, markers: &[usize]) -> SplitResult {
        self.split_with_edits(text, markers, &[])
    }

    /// markers 强制加边界;suppressed 抑制边界:切分完成后,byte_start 命中 suppressed 的章并入前一章。
    /// byte_start == 0 的 suppressed 忽略;不存在的 offset 也是 noop。
    /// 旧 split_with_markers 默认实现即 `split_with_edits(text, markers, &[])`。
    fn split_with_edits(
        &self,
        text: &str,
        markers: &[usize],
        suppressed: &[usize],
    ) -> SplitResult;
}

// 第N章 / 第N回（匹配整行作为标题）
static RE_CHAPTER_CN: Lazy<Regex> = Lazy::new(|| {
    // 行内水平空白(空格/制表/全角空格),不吃 \n;chapter byte_start 应对齐到行内"第"字位置,
    // 否则前端的 line.byte_start(行起点)与 chapter.byte_start 差 1 byte,合并后点剪刀不会清 suppressed。
    Regex::new(r"(?m)^[ \t　]*第[一二三四五六七八九十百千零〇\d]+[章回][^\n]*$").unwrap()
});
// Chapter N
static RE_CHAPTER_EN: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?m)^[ \t　]*Chapter\s+\d+[^\n]*$").unwrap()
});
// 卷N
static RE_VOLUME: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?m)^[ \t　]*卷[一二三四五六七八九十\d]+[^\n]*$").unwrap()
});
// 书名（X）/ （X）：小说常用「章节名（X）」结构。允许 ≤40 chars 的标题前缀
// (书名 / 篇名 + 冒号),整行收尾在「(X)」,前后只有水平空白(允许尾随 \r,
// 因为 CRLF 文件中章节行常以 \r 收尾)。区分于正文里的「(一)个人」之类:
// 这类句子含标点(,/。),不会让整行只到 (X) 就结束。
static RE_CHAPTER_PCN: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r"(?m)^[ \t　]*[^\n]{0,40}[（(][一二三四五六七八九十百千零〇\d]+[）)][ \t　\r]*$",
    )
    .unwrap()
});
// 多个连续空行(≥2)作段间分隔。兼容 CRLF:段落分隔处可能形如 \r\r\n / \r\n\r\n,
// 不能只盯 \n\n。
static RE_BLANK_LINE: Lazy<Regex> = Lazy::new(|| Regex::new(r"\r?\n[ \t]*\r?\n+").unwrap());

#[derive(Debug, Default, Clone, Copy)]
pub struct DefaultSplitter;

fn first_line_title(text: &str) -> String {
    let trimmed = text.trim_start();
    let first_line = trimmed.lines().next().unwrap_or("").trim();
    if first_line.is_empty() {
        "(无标题)".to_string()
    } else {
        first_line.to_string()
    }
}

fn auto_matches_in(text: &str) -> Vec<(usize, usize, String)> {
    let mut matches: Vec<(usize, usize, String)> = Vec::new();
    for re in [&RE_CHAPTER_CN, &RE_CHAPTER_EN, &RE_VOLUME, &RE_CHAPTER_PCN] {
        for m in re.find_iter(text) {
            matches.push((m.start(), m.end(), m.as_str().trim().to_string()));
        }
    }
    matches.sort_by_key(|(pos, _, _)| *pos);
    matches
}

impl ChapterSplitter for DefaultSplitter {
    fn split(&self, text: &str) -> SplitResult {
        if text.trim().is_empty() {
            return SplitResult { chapters: vec![] };
        }
        let matches = auto_matches_in(text);
        if matches.is_empty() {
            // RE_BLANK_LINE.split 拿不到 offset,改用 find_iter 计算真实起止
            let mut chapters: Vec<ParsedChapter> = Vec::new();
            let mut cursor = 0usize;
            for m in RE_BLANK_LINE.find_iter(text) {
                let seg_start = cursor;
                let seg_end = m.start();
                if seg_start < seg_end {
                    let s = &text[seg_start..seg_end];
                    if !s.trim().is_empty() {
                        chapters.push(ParsedChapter {
                            title: first_line_title(s),
                            content: s.trim().to_string(),
                            word_count: word_count(s),
                            byte_start: seg_start,
                            byte_end: seg_end,
                        });
                    }
                }
                cursor = m.end();
            }
            // 末段
            if cursor < text.len() {
                let s = &text[cursor..];
                if !s.trim().is_empty() {
                    chapters.push(ParsedChapter {
                        title: first_line_title(s),
                        content: s.trim().to_string(),
                        word_count: word_count(s),
                        byte_start: cursor,
                        byte_end: text.len(),
                    });
                }
            }
            return SplitResult { chapters };
        }
        let mut chapters: Vec<ParsedChapter> = Vec::new();
        for (i, (pos, end, title)) in matches.iter().enumerate() {
            let next_pos = matches.get(i + 1).map(|(p, _, _)| *p).unwrap_or(text.len());
            let content = &text[*end..next_pos];
            let trimmed = content.trim_start_matches('\n').trim_end().to_string();
            let wc = word_count(title) + word_count(content);
            chapters.push(ParsedChapter {
                title: title.clone(),
                content: trimmed,
                word_count: wc,
                byte_start: *pos,
                byte_end: next_pos,
            });
        }
        // head(首个「第N章」之前的内容)按元数据丢弃,不作为首章。
        // 极少数「序章/楔子」类首章可由用户在 Chapters.vue 加 marker 救回。
        SplitResult { chapters }
    }

    fn split_with_edits(
        &self,
        text: &str,
        markers: &[usize],
        suppressed: &[usize],
    ) -> SplitResult {
        if text.trim().is_empty() {
            return SplitResult { chapters: vec![] };
        }
        if markers.is_empty() {
            // 纯自动切,直接复用 split 再做抑制合并
            let mut result = self.split(text);
            merge_suppressed(&mut result.chapters, suppressed, text);
            return result;
        }
        // 构造分段边界:0 + 合法 markers + text.len()
        let mut bounds: Vec<usize> = Vec::with_capacity(markers.len() + 2);
        bounds.push(0);
        for &m in markers {
            if m > 0 && m < text.len() {
                bounds.push(m);
            }
        }
        bounds.push(text.len());
        bounds.sort();
        bounds.dedup();

        let mut all_chapters: Vec<ParsedChapter> = Vec::new();
        for window in bounds.windows(2) {
            let (start, end) = (window[0], window[1]);
            if start >= end {
                continue;
            }
            let segment = &text[start..end];
            let sub_matches = auto_matches_in(segment);
            // 窗口内章节必须按 byte 顺序 append,head 章节放在本窗口首位;
            // 不可直接 all_chapters.insert(0, head),会把前面窗口已追加的章节挪到 head 之后。
            // head(窗口内首个标题之前的内容)按元数据丢弃,不作为首章。
            let mut window_chapters: Vec<ParsedChapter> = Vec::new();
            if sub_matches.is_empty() {
                let trimmed = segment.trim();
                if !trimmed.is_empty() {
                    window_chapters.push(ParsedChapter {
                        title: first_line_title(segment),
                        content: trimmed.to_string(),
                        word_count: word_count(segment),
                        byte_start: start,
                        byte_end: end,
                    });
                }
            } else {
                for (i, (sub_pos, sub_end, title)) in sub_matches.iter().enumerate() {
                    let next_pos = sub_matches
                        .get(i + 1)
                        .map(|(p, _, _)| *p)
                        .unwrap_or(segment.len());
                    let content = &segment[*sub_end..next_pos];
                    let trimmed = content.trim_start_matches('\n').trim_end().to_string();
                    window_chapters.push(ParsedChapter {
                        title: title.clone(),
                        content: trimmed,
                        word_count: word_count(title) + word_count(content),
                        byte_start: start + sub_pos,
                        byte_end: start + next_pos,
                    });
                }
            }
            all_chapters.extend(window_chapters);
        }
        merge_suppressed(&mut all_chapters, suppressed, text);
        SplitResult { chapters: all_chapters }
    }
}

/// suppressed 处理:byte_start 命中的章并入前一章;byte_start == 0 / 不存在的 offset 忽略。
/// 连续 suppress 级联合并。byte_end 在原文本合法。
fn merge_suppressed(chapters: &mut Vec<ParsedChapter>, suppressed: &[usize], text: &str) {
    if suppressed.is_empty() || chapters.is_empty() {
        return;
    }
    let suppressed_set: std::collections::HashSet<usize> =
        suppressed.iter().copied().filter(|&b| b > 0).collect();
    if suppressed_set.is_empty() {
        return;
    }
    let mut merged: Vec<ParsedChapter> = Vec::with_capacity(chapters.len());
    for ch in chapters.drain(..) {
        if suppressed_set.contains(&ch.byte_start) {
            // 必须有前一章能并入;否则(理论上 byte_start==0 时已过滤)保留
            if let Some(prev) = merged.last_mut() {
                prev.byte_end = ch.byte_end;
                prev.word_count = prev.word_count.saturating_add(ch.word_count);
                // content 拼接方便前端展示;byte 范围仍以 byte_start/byte_end 为准。
                // 末章覆盖文末时,text 截到 text.len() 保护 panic。
                let end = ch.byte_end.min(text.len());
                let slice = text.get(ch.byte_start..end)
                    .expect("suppress merge: byte range out of bounds — data integrity violation");
                prev.content.push('\n');
                prev.content.push_str(slice);
                continue;
            }
        }
        merged.push(ch);
    }
    *chapters = merged;
}