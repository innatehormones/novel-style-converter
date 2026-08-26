import { test, expect } from '@playwright/test';

const SAMPLE_TEXT = [
  '第一章：开篇',
  'body line 一',
  'body line 二',
  'body line 三',
  'body line 四',
  '第二章今世只想生孩子',
  'body line 1 of chapter 2',
  'body line 2 of chapter 2',
  'body line 3 of chapter 2',
  'body line 4 of chapter 2',
  'body line 5 of chapter 2',
  '第三章：误会',
  'body line 1 of chapter 3',
  'body line 2 of chapter 3',
].join('\n');

const SAMPLE_SEGMENTS = [
  { title: '第一章：开篇', content: 'body line 一\nbody line 二\nbody line 三\nbody line 四', word_count: 18 },
  { title: '第二章今世只想生孩子', content: 'body line 1 of chapter 2\nbody line 2 of chapter 2\nbody line 3 of chapter 2\nbody line 4 of chapter 2\nbody line 5 of chapter 2', word_count: 60 },
  { title: '第三章：误会', content: 'body line 1 of chapter 3\nbody line 2 of chapter 3', word_count: 24 },
];

const MOCK_INIT_SCRIPT = `
  window.__console_lines__ = [];
  const _origLog = console.log;
  const _origErr = console.error;
  const _origWarn = console.warn;
  const cap = (...args) => {
    try {
      const line = args.map(a => {
        if (a === undefined) return 'undefined';
        if (a === null) return 'null';
        if (typeof a === 'string') return a;
        try { return JSON.stringify(a); } catch { return String(a); }
      }).join(' ');
      window.__console_lines__.push({ kind: 'log', line });
    } catch (e) {}
    _origLog.apply(console, args);
  };
  cap.error = (...args) => {
    try {
      const line = args.map(a => {
        if (a === undefined) return 'undefined';
        if (a === null) return 'null';
        if (typeof a === 'string') return a;
        try { return JSON.stringify(a); } catch { return String(a); }
      }).join(' ');
      window.__console_lines__.push({ kind: 'error', line });
    } catch (e) {}
    _origErr.apply(console, args);
  };
  cap.warn = (...args) => {
    try {
      const line = args.map(a => {
        if (a === undefined) return 'undefined';
        if (a === null) return 'null';
        if (typeof a === 'string') return a;
        try { return JSON.stringify(a); } catch { return String(a); }
      }).join(' ');
      window.__console_lines__.push({ kind: 'warn', line });
    } catch (e) {}
    _origWarn.apply(console, args);
  };
  console.log = cap;
  console.error = cap.error;
  console.warn = cap.warn;

  // Mock Tauri IPC
  window.__TAURI_INTERNALS__ = {
    invoke: async (cmd, args) => {
      console.log('[mock-ipc]', cmd, JSON.stringify(args));
      if (cmd === 'get_upload_text') return ${JSON.stringify(SAMPLE_TEXT)};
      if (cmd === 'get_upload') return { id: args.id, filename: 'sample.txt', size: 100 };
      if (cmd === 'find_data_asset_by_upload') return [];
      if (cmd === 'list_chapter_segments') return ${JSON.stringify(SAMPLE_SEGMENTS)};
      if (cmd === 'list_committed_segments') return [];
      if (cmd === 'commit_data_asset') return 1;
      throw new Error('mock not implemented: ' + cmd);
    },
  };
  window.__TAURI_OS_PLUGIN_INTERNALS__ = { platform: 'web' };
`;

test.skip('stopped batch: append 2 chapters triggers running transition', async ({ page }) => {
  // 1. 启动 app + mock IPC 让某 batch 处于 stopped 状态(含 3 章 done)
  // 2. 点 workflow 行的「补充章节」
  // 3. 选 2 章未转换的
  // 4. 点「确认补充并执行」
  // 5. 断言:batch status 变 running;新章节行出现并转 done
});

test.skip('running batch: append button hidden', async ({ page }) => {
  // batch.status='running' 时 actions 列不应显示「补充章节」按钮
});