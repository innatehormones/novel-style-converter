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

test('parse page 章 button click toggles chapter boundary and splits chapter', async ({ page }) => {
  await page.addInitScript(MOCK_INIT_SCRIPT);

  page.on('pageerror', (err) => {
    console.log('[pageerror]', err.message, err.stack);
  });

  await page.goto('/library/upload/1/parse');
  await page.waitForLoadState('networkidle');

  // Wait for editor to render
  await page.waitForSelector('.cm-editor', { timeout: 10000 });
  // Wait for 章 buttons to appear
  await page.waitForSelector('.cm-marker-stamp', { timeout: 10000 });
  const stampCount = await page.locator('.cm-marker-stamp').count();
  console.log('[test] stamp count:', stampCount);

  // Find a body line (line 2, 0-based index 1 in document "第一章：开篇\nbody line 一\n...")
  // Lines (0-based): 0=第一章, 1=body line 一, 2=body line 二, 3=body line 三, 4=body line 四
  // 5=第二章..., 6=body line 1 of chapter 2, ...
  // Click 章 on line 2 (body line 二) — should split chapter 1.
  // The 章 buttons are rendered per line; index 2 should be the line "body line 二".
  const stamps = page.locator('.cm-marker-stamp');
  await stamps.nth(2).click();
  console.log('[test] clicked stamp #2');

  // Wait for debouncedRecompute (200ms) + some processing
  await page.waitForTimeout(800);

  // Check the chapter list count in the left pane
  const chapterList = await page.locator('.seg-row').count();
  console.log('[test] chapter rows count after click:', chapterList);

  // Dump all console
  const lines = await page.evaluate(() => window.__console_lines__ || []);
  console.log('[test] CONSOLE LINES:');
  for (const l of lines) console.log(`  [${l.kind}] ${l.line}`);

  // Dump HTML for debugging
  const editorHtml = await page.locator('.cm-editor').innerHTML();
  console.log('[test] editor HTML length:', editorHtml.length);

  await page.screenshot({ path: 'parse-zhang-click.png', fullPage: true });
});
test('parse page: 章 on a merged chapter title restores that chapter', async ({ page }) => {
  await page.addInitScript(MOCK_INIT_SCRIPT);

  page.on('pageerror', (err) => {
    console.log('[pageerror]', err.message, err.stack);
  });

  await page.goto('/library/upload/1/parse');
  await page.waitForLoadState('networkidle');

  await page.waitForSelector('.cm-editor', { timeout: 10000 });
  await page.waitForSelector('.cm-marker-stamp', { timeout: 10000 });

  // Sanity: three chapter rows out of the box.
  await expect(page.locator('.seg-row')).toHaveCount(3);

  // User clicks 章 on chapter 2 title line to "merge" chapter 2 into chapter 1.
  // (Replaces the removed 并入上一章 button: in the stack model, removing
  // chapter 2 from chapterSplits has the same effect — its body concatenates
  // into chapter 1.) Stamps render in document order; nth(5) is chapter 2's title.
  await page.locator('.cm-marker-stamp').nth(5).click();
  await page.waitForTimeout(600);

  // Left list now has 2 chapters (Ch1 + Ch2 merged, Ch3).
  await expect(page.locator('.pane-title').first()).toContainText('章节列表(2)');

  // User clicks 章 on chapter 2 title line again to restore chapter 2.
  await page.locator('.cm-marker-stamp').nth(5).click();

  // Pre-fix: pane title stayed at 2 chapters because onBoundaryToggle
  // fed a line-number key into a store keyed by seg.content (always false),
  // so the chapter stayed suppressed. Post-fix: chapter 2 is back.
  await expect(page.locator('.pane-title').first()).toContainText('章节列表(3)');
  // Verify chapter 2 row is back (title input has the expected value).
  // Use a robust selector that searches by value across all inputs.
  await expect(page.locator('input.seg-title[value*="第二章今世只想生孩子"]').first()).toBeVisible();
});

