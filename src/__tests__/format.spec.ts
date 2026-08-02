import { describe, it, expect } from 'vitest';
import { formatSize, formatTime, formatWordCount } from '../utils/format';

describe('format helpers', () => {
  describe('formatSize', () => {
    it('under 1024 B uses raw B', () => {
      expect(formatSize(0)).toBe('0 B');
      expect(formatSize(1023)).toBe('1023 B');
    });
    it('KB uses 1 decimal', () => {
      expect(formatSize(1024)).toBe('1.0 KB');
      expect(formatSize(1536)).toBe('1.5 KB');
    });
    it('MB uses 2 decimals', () => {
      expect(formatSize(1024 * 1024)).toBe('1.00 MB');
    });
  });

  describe('formatTime', () => {
    it('RFC3339 → YYYY-MM-DD HH:mm', () => {
      expect(formatTime('2026-07-26T15:04:32+00:00')).toBe('2026-07-26 15:04');
    });
  });

  /// 之前 Library.vue 用 "1.2 万字" / parse.vue 用 "12,000 字",作者在两个页面
  /// 看到的同一本书数字不一致。这里锁住:统一千分位 + "字",不接受 "万字" 截断。
  describe('formatWordCount', () => {
    it('千分位 + 字', () => {
      expect(formatWordCount(0)).toBe('0 字');
      expect(formatWordCount(123)).toBe('123 字');
      expect(formatWordCount(12345)).toBe('12,345 字');
      expect(formatWordCount(1234567)).toBe('1,234,567 字');
    });
    it('负数 / NaN / Infinity 显示 "?" 而不是 "NaN 字" 之类的串', () => {
      expect(formatWordCount(-1)).toBe('?');
      expect(formatWordCount(NaN)).toBe('?');
      expect(formatWordCount(Infinity)).toBe('?');
    });
  });
});