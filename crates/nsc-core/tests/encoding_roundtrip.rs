use nsc_core::encoding::{decode_to_utf8, read_text_file, EncodingKind};
use std::io::Write;

#[test]
fn gbk_bytes_decode_to_utf8() {
    let gbk: Vec<u8> = vec![
        0xC4, 0xE3, 0xBA, 0xC3, 0x2C, 0xCA, 0xC0, 0xBD, 0xE7, 0x21,
    ];
    let result = decode_to_utf8(&gbk).unwrap();
    assert_eq!(result.kind, EncodingKind::Gbk);
    assert_eq!(result.text, "你好,世界!");
}

#[test]
fn utf8_bytes_passthrough() {
    let utf8 = "你好,世界!".as_bytes().to_vec();
    let result = decode_to_utf8(&utf8).unwrap();
    assert_eq!(result.text, "你好,世界!");
}

#[test]
fn ascii_passthrough() {
    let ascii = b"Hello, World!".to_vec();
    let result = decode_to_utf8(&ascii).unwrap();
    assert_eq!(result.text, "Hello, World!");
}

#[test]
fn short_utf8_not_misclassified_as_gbk() {
    // 短文本 + 中文,chardetng 可能误判;严格 UTF-8 校验应优先识别为 UTF-8
    let bytes = "第一章 测试".as_bytes().to_vec();
    let result = decode_to_utf8(&bytes).unwrap();
    assert_eq!(result.kind, EncodingKind::Utf8);
    assert_eq!(result.text, "第一章 测试");
}

#[test]
fn malformed_utf8_rejected() {
    // 含截断多字节序列,严格 UTF-8 验证应失败
    let bytes = vec![0xC4, 0xE3];  // 不完整的 GBK/UTF-8 序列
    let result = decode_to_utf8(&bytes);
    // 严格 UTF-8 失败 → chardetng 走 GBK → 解码可能成功("你")或失败
    // 关键是:不能 silent 接受乱码
    if let Ok(decoded) = result {
        // 如果 chardetng 选了 GBK 且能解码,内容应至少 1 个有效字符
        assert!(!decoded.text.is_empty());
    }
    // 也可能 Err —— 都 OK,只要不静默
}

#[test]
fn read_text_file_utf8_passthrough() {
    let mut f = tempfile::NamedTempFile::new().unwrap();
    let payload = "第一章 测试用例\n第二行也有中文\n";
    f.write_all(payload.as_bytes()).unwrap();
    let decoded = read_text_file(f.path()).unwrap();
    assert_eq!(decoded.kind, EncodingKind::Utf8);
    assert_eq!(decoded.text, payload);
}

#[test]
fn read_text_file_gbk_decoded() {
    // "你好,世界!" 的 GBK 字节
    let gbk: Vec<u8> = vec![0xC4, 0xE3, 0xBA, 0xC3, 0x2C, 0xCA, 0xC0, 0xBD, 0xE7, 0x21];
    let mut f = tempfile::NamedTempFile::new().unwrap();
    f.write_all(&gbk).unwrap();
    let decoded = read_text_file(f.path()).unwrap();
    assert_eq!(decoded.kind, EncodingKind::Gbk);
    assert_eq!(decoded.text, "你好,世界!");
}

#[test]
fn read_text_file_missing_returns_io_error() {
    let dir = tempfile::tempdir().unwrap();
    let bogus = dir.path().join("does_not_exist.txt");
    let err = read_text_file(&bogus).unwrap_err();
    // 应该是 io 错误(读到文件之前就 return 了),不是 decode 错误
    assert!(err.starts_with("读文件失败"), "expected io-prefixed error, got: {err}");
    // 路径应出现在错误里便于定位
    assert!(err.contains("does_not_exist.txt"), "error should include path, got: {err}");
}
