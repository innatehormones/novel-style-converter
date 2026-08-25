import { describe, expect, it } from 'vitest';
import {
  countChapterChars,
  isVisuallyEmptyLine,
  stripInvisibles,
  stripTrailingInvisibles,
} from '../utils/splitChapters';

describe('isVisuallyEmptyLine', () => {
  it('plain whitespace counts as visually empty', () => {
    expect(isVisuallyEmptyLine('')).toBe(true);
    expect(isVisuallyEmptyLine('   ')).toBe(true);
    expect(isVisuallyEmptyLine('\t\n')).toBe(true);
  });
  it('ZWSP / BOM only counts as visually empty', () => {
    expect(isVisuallyEmptyLine('\u{200B}')).toBe(true);
    expect(isVisuallyEmptyLine('\u{FEFF}')).toBe(true);
    expect(isVisuallyEmptyLine('  \u{200B}\u{FEFF}  ')).toBe(true);
  });
  it('non-empty content is not visually empty', () => {
    expect(isVisuallyEmptyLine('  正文  ')).toBe(false);
    expect(isVisuallyEmptyLine('\u{200B}a')).toBe(false);
  });
});

describe('countChapterChars', () => {
  it('whitespace is not counted', () => {
    expect(countChapterChars('  a b\nc')).toBe(3);
  });
  it('empty string returns 0', () => {
    expect(countChapterChars('')).toBe(0);
  });
  it('chinese + english + digits', () => {
    expect(countChapterChars('你好 world 123')).toBe(10);
  });
});

describe('stripInvisibles / stripTrailingInvisibles', () => {
  it('stripInvisibles trims whitespace + ZWSP/BOM on both sides', () => {
    expect(stripInvisibles('  \u{200B}第1章\u{FEFF} ')).toBe('第1章');
  });
  it('stripTrailingInvisibles keeps leading whitespace', () => {
    expect(stripTrailingInvisibles('  正文\n\n')).toBe('  正文');
  });
});
