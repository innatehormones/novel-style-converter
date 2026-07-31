use nsc_core::splitter::{ChapterSplitter, DefaultSplitter};

// ===== split_with_edits: markers + suppressed(抑制 = 与上一章合并) =====

#[test]
fn suppressed_boundary_merges_into_previous_chapter() {
    let text = "前言\n\n第一章 开始\n正文一\n\n第二章 继续\n正文二\n\n第三章 结束\n正文三\n";
    let auto = DefaultSplitter.split_with_edits(text, &[], &[]);
    assert!(auto.chapters.len() >= 3);
    // 抑制"第二章"的起点 → 第二章并入第一章
    let ch2_start = auto
        .chapters
        .iter()
        .find(|c| c.title.contains("第二章"))
        .unwrap()
        .byte_start;
    let before = auto.chapters.len();
    let r = DefaultSplitter.split_with_edits(text, &[], &[ch2_start]);
    assert_eq!(r.chapters.len(), before - 1);
    // 合并后前一章的 byte_end 覆盖到原第二章的 byte_end
    let merged = r
        .chapters
        .iter()
        .find(|c| c.title.contains("第一章"))
        .unwrap();
    let old_ch2 = auto
        .chapters
        .iter()
        .find(|c| c.title.contains("第二章"))
        .unwrap();
    assert_eq!(merged.byte_end, old_ch2.byte_end);
}

#[test]
fn suppress_at_zero_is_ignored() {
    let text = "第一章 A\n正文\n第二章 B\n正文\n";
    let auto = DefaultSplitter.split_with_edits(text, &[], &[]);
    let r = DefaultSplitter.split_with_edits(text, &[], &[0]);
    assert_eq!(r.chapters.len(), auto.chapters.len());
}

#[test]
fn suppress_nonexistent_boundary_is_noop() {
    let text = "第一章 A\n正文\n第二章 B\n正文\n";
    let auto = DefaultSplitter.split_with_edits(text, &[], &[]);
    let r = DefaultSplitter.split_with_edits(text, &[], &[999999]);
    assert_eq!(r.chapters.len(), auto.chapters.len());
}

#[test]
fn marker_and_suppress_combine() {
    // 用 marker 在无标记文本处切一刀,再 suppress 掉,应回到单章
    let text = "AAAA\nBBBB\nCCCC\nDDDD\n";
    let with_marker = DefaultSplitter.split_with_edits(text, &[10], &[]);
    assert_eq!(with_marker.chapters.len(), 2);
    let r = DefaultSplitter.split_with_edits(text, &[10], &[10]);
    assert_eq!(r.chapters.len(), 1);
    assert_eq!(r.chapters[0].byte_end, text.len());
}

#[test]
fn consecutive_suppresses_cascade_merge() {
    let text = "前言\n\n第一章 A\n正文一\n\n第二章 B\n正文二\n\n第三章 C\n正文三\n";
    let auto = DefaultSplitter.split_with_edits(text, &[], &[]);
    assert!(auto.chapters.len() >= 3);
    // 同时抑制第二章和第三章的起点 → 三章并成一章
    let ch2_start = auto
        .chapters
        .iter()
        .find(|c| c.title.contains("第二章"))
        .unwrap()
        .byte_start;
    let ch3_start = auto
        .chapters
        .iter()
        .find(|c| c.title.contains("第三章"))
        .unwrap()
        .byte_start;
    let r = DefaultSplitter.split_with_edits(text, &[], &[ch2_start, ch3_start]);
    // 第一章 + 两章合并 → 至少少两章
    let ch1_end = auto
        .chapters
        .iter()
        .find(|c| c.title.contains("第一章"))
        .unwrap()
        .byte_end;
    let ch3_end = auto
        .chapters
        .iter()
        .find(|c| c.title.contains("第三章"))
        .unwrap()
        .byte_end;
    // 级联合并后只剩"第一章"那一章,byte_end 覆盖到原第三章末尾
    assert!(r.chapters.len() < auto.chapters.len());
    let merged = r
        .chapters
        .iter()
        .find(|c| c.title.contains("第一章"))
        .unwrap();
    // 第一章 byte_end 起码扩展过 ch1_end
    assert!(merged.byte_end >= ch1_end);
    // 整体范围覆盖到原文末尾
    assert_eq!(r.chapters.last().unwrap().byte_end, ch3_end);
}

#[test]
fn suppress_merges_word_count() {
    let text = "第一章 A\n正文甲\n\n第二章 B\n正文乙\n";
    let auto = DefaultSplitter.split_with_edits(text, &[], &[]);
    let ch1 = auto
        .chapters
        .iter()
        .find(|c| c.title.contains("第一章"))
        .unwrap();
    let ch2 = auto
        .chapters
        .iter()
        .find(|c| c.title.contains("第二章"))
        .unwrap();
    let r = DefaultSplitter.split_with_edits(text, &[], &[ch2.byte_start]);
    let merged = r
        .chapters
        .iter()
        .find(|c| c.title.contains("第一章"))
        .unwrap();
    assert_eq!(
        merged.word_count,
        ch1.word_count + ch2.word_count,
        "word_count 应为两章之和"
    );
}

#[test]
fn markers_force_split_at_position() {
    let text = "AAA\nBBB\nCCC\nDDD\nEEE\nFFF\n";
    let markers = vec![8usize, 16usize];
    let r = DefaultSplitter.split_with_markers(text, &markers);
    assert_eq!(r.chapters.len(), 3);
}

#[test]
fn empty_markers_falls_back_to_auto() {
    let text = "第一章 A\n正文\n第二章 B\n正文\n";
    let r = DefaultSplitter.split_with_markers(text, &[]);
    assert!(r.chapters.iter().any(|c| c.title.contains("第一章")));
    assert!(r.chapters.iter().any(|c| c.title.contains("第二章")));
}

#[test]
fn marker_aligned_with_title() {
    let text = "前言\n\n第一章 开始\n正文\n\n第二章 继续\n正文2\n";
    let chapter2_pos = text.find("第二章").unwrap();
    let r = DefaultSplitter.split_with_markers(text, &[chapter2_pos]);
    let titles: Vec<&str> = r.chapters.iter().map(|c| c.title.as_str()).collect();
    assert!(titles.iter().any(|t| t.contains("第二章")));
}

#[test]
fn text_without_marker_yields_single_chapter() {
    let text = "无章节标记的纯文本内容";
    let r = DefaultSplitter.split_with_markers(text, &[]);
    assert_eq!(r.chapters.len(), 1);
}

#[test]
fn out_of_range_marker_ignored() {
    let text = "第一章 A\n正文";
    let r = DefaultSplitter.split_with_markers(text, &[10000]);
    assert_eq!(r.chapters.len(), 1);
    assert!(r.chapters[0].title.contains("第一章"));
}

#[test]
fn zero_marker_ignored() {
    let text = "第一章 A\n正文\n第二章 B\n正文\n";
    let r = DefaultSplitter.split_with_markers(text, &[0]);
    assert_eq!(r.chapters.len(), 2);
}

#[test]
fn byte_offsets_align_with_source_text() {
    let text = "前言\n\n第一章 开始\n正文一\n\n第二章 继续\n正文二\n";
    let r = DefaultSplitter.split(text);
    assert!(!r.chapters.is_empty());
    // 每章 range 切原文应包含该章标题(无标题 head 跳过)
    for c in &r.chapters {
        if c.title == "(无标题)" {
            continue;
        }
        let slice = &text[c.byte_start..c.byte_end];
        assert!(
            slice.contains(c.title.as_str()),
            "chapter {:?} range mismatch: slice={:?}",
            c.title,
            slice
        );
    }
    // ranges 连续覆盖且不重叠(按 byte_start 排序后)
    let mut sorted: Vec<_> = r.chapters.iter().collect();
    sorted.sort_by_key(|c| c.byte_start);
    for w in sorted.windows(2) {
        assert!(w[0].byte_end <= w[1].byte_start);
    }
    // 最后一章覆盖到文末
    assert_eq!(sorted.last().unwrap().byte_end, text.len());
}

#[test]
fn marker_byte_offsets_absolute_not_segment_relative() {
    let text = "AAAA\nBBBB\nCCCC\nDDDD\n";
    let markers = vec![10usize];
    let r = DefaultSplitter.split_with_markers(text, &markers);
    assert_eq!(r.chapters.len(), 2);
    assert_eq!(r.chapters[0].byte_start, 0);
    assert_eq!(r.chapters[1].byte_start, 10);
    assert_eq!(r.chapters[1].byte_end, text.len());
}

// ===== head 章节插入顺序回归测试 =====

#[test]
fn marker_inside_chapter_keeps_byte_order() {
    // 用户场景:合并第2章后,在第2章内某位置("正文二"行)重加 marker。
    // 此时 suppressed 仍保留 chapter 2 原起点,会被合并到前一章;marker 落在章内则
    // 切出 head 章节(如"正文二")。关键回归:head 不能跑到列表首位。
    let text = "前言\n\n第一章 开始\n正文一\n\n第二章 继续\n正文二\n\n第三章 结束\n正文三\n";
    let auto = DefaultSplitter.split_with_edits(text, &[], &[]);
    let ch2_start = auto
        .chapters
        .iter()
        .find(|c| c.title.contains("第二章"))
        .unwrap()
        .byte_start;
    let pos_inside = text.find("正文二").unwrap();
    assert_ne!(pos_inside, ch2_start);
    let r = DefaultSplitter.split_with_edits(text, &[pos_inside], &[ch2_start]);
    // 必须按 byte_start 升序排列,且 byte_end > byte_start
    let mut prev = 0usize;
    for c in &r.chapters {
        assert!(
            c.byte_start >= prev,
            "章节乱序: prev={} byte_start={} title={:?}",
            prev,
            c.byte_start,
            c.title
        );
        assert!(
            c.byte_end > c.byte_start,
            "章节空: byte_start={} byte_end={} title={:?}",
            c.byte_start,
            c.byte_end,
            c.title
        );
        prev = c.byte_end;
    }
}

#[test]
fn marker_at_collapsed_position_clears_suppress_and_restores() {
    // 合并第2章后,在 ch2_start 处重加 marker,应完全恢复原 4 章结构。
    // ch2_start 必须对齐到行内"第"字位置(不吃前导 \n),这样前端的
    // line.byte_start 才能与 ch2_start 相等,addMarker 才能正确清掉 suppressed。
    let text = "前言\n\n第一章 开始\n正文一\n\n第二章 继续\n正文二\n\n第三章 结束\n正文三\n";
    let auto = DefaultSplitter.split_with_edits(text, &[], &[]);
    let ch2_start = auto
        .chapters
        .iter()
        .find(|c| c.title.contains("第二章"))
        .unwrap()
        .byte_start;
    // 模拟前端 line.byte_start(同 splitter byte_start,都指向"第"字)
    let line_byte_start = ch2_start;
    let restored = DefaultSplitter.split_with_edits(text, &[line_byte_start], &[]);
    // 与原始 auto 章节内容(标题 + byte 范围)完全一致
    assert_eq!(restored.chapters.len(), auto.chapters.len());
    for (got, exp) in restored.chapters.iter().zip(auto.chapters.iter()) {
        assert_eq!(got.byte_start, exp.byte_start);
        assert_eq!(got.byte_end, exp.byte_end);
        assert_eq!(got.title, exp.title);
    }
}

#[test]
fn chapter_byte_start_aligns_with_first_title_char_no_leading_newline_consumed() {
    // 回归:正则改为不消耗前导 \n 之后,chapter.byte_start 必须等于"第"字位置,
    // 否则 addMarker(line.byte_start) 永远清不掉 suppressed(差 1 byte)。
    // 文本中有空行作为章节间隔(空行的 \n 必须被算到前一章,不能算到下一章)。
    let text = "前言\n\n第一章 开始\n正文一\n\n第二章 继续\n正文二\n\n第三章 结束\n正文三\n";
    let r = DefaultSplitter.split(text);
    let ch2 = r
        .chapters
        .iter()
        .find(|c| c.title.contains("第二章"))
        .unwrap();
    // "第" 字位置:空行后 byte 36 是"第"
    let pos_di = text.find("第二章 继续").unwrap();
    assert_eq!(
        ch2.byte_start, pos_di,
        "chapter.byte_start 必须等于'第'字位置(line.byte_start)"
    );
}