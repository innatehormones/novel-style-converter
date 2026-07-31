/// 字数 = alphanumeric 字符数(汉字 + 字母 + 数字)。空白 / 标点 / 换行不计。
///
/// 旧版本按"非空白连续段"计数,中文段落无空格时整段被算成 1 个 word,
/// 用户反馈"万字一章显示 200 字"。改成字符级计数:中文每字 1、英文每字母 1、
/// 数字每字 1,标点 / 空白 / 控制字符不计。跟 Word / WPS 的"字数"统计一致。
pub fn count(s: &str) -> i32 {
    s.chars().filter(|c| c.is_alphanumeric()).count() as i32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_and_whitespace_only() {
        assert_eq!(count(""), 0);
        assert_eq!(count("   "), 0);
        assert_eq!(count("\n\t　 "), 0);
    }

    #[test]
    fn chinese_runs() {
        // 字符级计数:每个汉字算 1。
        assert_eq!(count("第一章 开始"), 5);
        assert_eq!(count("正文一\n\n正文二"), 6);
        // 万字一章 → 10000,不再退化成 1。
        assert_eq!(count(&"字".repeat(10_000)), 10_000);
    }

    #[test]
    fn english_runs() {
        // 英文字母按字符计(不是 word);"hello" = 5。
        assert_eq!(count("hello world"), 10);
        // apostrophe / 空格都不算 alphanumeric
        assert_eq!(count("don't go there"), 11);
    }

    #[test]
    fn mixed_chinese_english() {
        assert_eq!(count("hello 中文"), 7);
        // 第1章(3 汉字 + 1 数字) + Chapter One(10 字母) = 14
        assert_eq!(count("第1章 Chapter One"), 13);
    }

    #[test]
    fn leading_and_trailing_whitespace() {
        assert_eq!(count("  hello  "), 5);
        assert_eq!(count("\n正文一\n"), 3);
    }

    #[test]
    fn punctuation_not_counted() {
        // 标点不算字数。
        assert_eq!(count("你好,世界!"), 4);
        assert_eq!(count("“引号”也算标点"), 6);
    }
}