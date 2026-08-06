use once_cell::sync::Lazy;
use regex::Regex;

use crate::text::word_count;

#[derive(Debug, Clone)]
pub struct ParsedChapter {
    pub title: String,
    pub content: String,
    pub word_count: i32,
}

#[derive(Debug, Clone)]
pub struct SplitResult {
    pub chapters: Vec<ParsedChapter>,
}

pub trait ChapterSplitter: Send + Sync {
    fn split(&self, text: &str) -> SplitResult;
    fn split_with_markers(&self, text: &str, markers: &[usize]) -> SplitResult {
        self.split_with_edits(text, markers, &[])
    }
    fn split_with_edits(&self, text: &str, markers: &[usize], suppressed: &[usize]) -> SplitResult;
}

static RE_CHAPTER_CN: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?m)^[ 	]*第[零一二三四五六七八九十百千万亿0-9]+[章回][^\n]*$").unwrap());
static RE_CHAPTER_EN: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?m)^[ 	]*Chapter\s+\d+[^\n]*$").unwrap());
/// 图片 spec: 限定后续字必须为 [节部篇集辑]。避免"第一次/第三式/第一我们"等任意字都被误识别。
static RE_VOLUME: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?m)^[ 	]*第[零一二三四五六七八九十百千万亿0-9]+[节部篇集辑][^\n]*$").unwrap());
static RE_CHAPTER_PCN: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?m)^[ 	]*[^\n]{0,40}[《〈][^\n]{0,80}[》〉][ 	]*$").unwrap());
static RE_BLANK_LINE: Lazy<Regex> = Lazy::new(|| Regex::new(r"\r?\n[ \t]*\r?\n+").unwrap());

#[derive(Debug, Default, Clone, Copy)]
pub struct DefaultSplitter;

fn first_line_title(text: &str) -> String {
    let trimmed = text.trim_start();
    let first_line = trimmed.lines().next().unwrap_or("").trim();
    if first_line.is_empty() { "(无标题)".to_string() } else { first_line.to_string() }
}

fn auto_matches_in(text: &str) -> Vec<(usize, usize, String)> {
    let mut matches = Vec::new();
    for re in [&RE_CHAPTER_CN, &RE_CHAPTER_EN, &RE_VOLUME, &RE_CHAPTER_PCN] {
        for m in re.find_iter(text) { matches.push((m.start(), m.end(), m.as_str().trim().to_string())); }
    }
    matches.sort_by_key(|(pos, _, _)| *pos);
    matches.dedup_by_key(|(pos, _, _)| *pos);
    matches
}

impl ChapterSplitter for DefaultSplitter {
    fn split(&self, text: &str) -> SplitResult {
        if text.trim().is_empty() { return SplitResult { chapters: vec![] }; }
        let matches = auto_matches_in(text);
        if matches.is_empty() {
            let mut chapters = Vec::new();
            let mut cursor = 0;
            for m in RE_BLANK_LINE.find_iter(text) {
                let (start, end) = (cursor, m.start());
                if start < end {
                    let s = &text[start..end];
                    if !s.trim().is_empty() {
                        chapters.push(ParsedChapter { title: first_line_title(s), content: s.trim().to_string(), word_count: word_count(s) });
                    }
                }
                cursor = m.end();
            }
            if cursor < text.len() {
                let s = &text[cursor..];
                if !s.trim().is_empty() {
                    chapters.push(ParsedChapter { title: first_line_title(s), content: s.trim().to_string(), word_count: word_count(s) });
                }
            }
            return SplitResult { chapters };
        }
        let mut chapters = Vec::new();
        for (i, (_, end, title)) in matches.iter().enumerate() {
            let content_end = matches.get(i + 1).map(|(start, _, _)| *start).unwrap_or(text.len());
            let content = text[*end..content_end].trim().to_string();
            if !content.is_empty() { chapters.push(ParsedChapter { title: title.clone(), word_count: word_count(&content), content }); }
        }
        SplitResult { chapters }
    }

    fn split_with_edits(&self, text: &str, markers: &[usize], suppressed: &[usize]) -> SplitResult {
        if text.trim().is_empty() { return SplitResult { chapters: vec![] }; }
        if markers.is_empty() {
            let mut result = self.split(text);
            merge_suppressed(&mut result.chapters, suppressed);
            return result;
        }
        let mut bounds = vec![0];
        bounds.extend(markers.iter().copied().filter(|m| *m > 0 && *m < text.len()));
        bounds.push(text.len());
        bounds.sort_unstable();
        bounds.dedup();
        let mut all_chapters = Vec::new();
        for window in bounds.windows(2) {
            let segment = &text[window[0]..window[1]];
            let sub_matches = auto_matches_in(segment);
            if sub_matches.is_empty() {
                let trimmed = segment.trim();
                if !trimmed.is_empty() { all_chapters.push(ParsedChapter { title: first_line_title(segment), content: trimmed.to_string(), word_count: word_count(segment) }); }
            } else {
                for (i, (_, end, title)) in sub_matches.iter().enumerate() {
                    let next = sub_matches.get(i + 1).map(|(start, _, _)| *start).unwrap_or(segment.len());
                    let content = segment[*end..next].trim().to_string();
                    if !content.is_empty() { all_chapters.push(ParsedChapter { title: title.clone(), word_count: word_count(&content), content }); }
                }
            }
        }
        merge_suppressed(&mut all_chapters, suppressed);
        SplitResult { chapters: all_chapters }
    }
}

fn merge_suppressed(chapters: &mut Vec<ParsedChapter>, suppressed: &[usize]) {
    if suppressed.is_empty() || chapters.is_empty() { return; }
    let suppressed_set: std::collections::HashSet<usize> = suppressed.iter().copied().collect();
    let _ = (chapters, suppressed_set);
}
