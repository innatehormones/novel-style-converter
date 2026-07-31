import { test, expect } from '@playwright/test';

// Tauri 应用在 npm run dev (dev server) 下不会调用 Rust 后端。
// 本 smoke 仅覆盖 web 端渲染。后端集成测试由 vitest store 测试覆盖。

test('library page mounts and shows empty state when no data', async ({ page }) => {
  await page.goto('/library');

  await expect(page.getByRole('heading', { name: 'Library' })).toBeVisible();
  await expect(page.getByRole('button', { name: '新增' })).toBeVisible();
});

test('new novel dialog opens and validates empty title', async ({ page }) => {
  await page.goto('/library');

  await page.getByRole('button', { name: '新增' }).click();

  const saveBtn = page.getByRole('button', { name: '保存' });
  await expect(saveBtn).toBeVisible();
  await expect(saveBtn).toBeDisabled();

  await page.getByLabel('标题').fill('测试小说');
  await expect(saveBtn).toBeEnabled();
});
