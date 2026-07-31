import { test } from '@playwright/test';

// Transform 结果查看页的端到端用例占位。
//
// 该流程依赖以下两件本仓库当前 CI 环境不具备的能力:
//   1. 一个 fake LLM endpoint（wiremock / httpbin / 自建 mock server），
//      用于让 JobQueue 跑完一次 transform 并产出 `Done` 的 transformation 行;
//   2. 真实 Tauri runtime（`pnpm tauri dev` 起的桌面壳），IPC 调用才会
//      真正打到 Rust 后端,前端组件才能拿到 `ChapterWithNovel` /
//      `Transformation` 等结构。
//
// 当前 `playwright.config.ts` 用 `pnpm run dev` 起 Vite dev server,
// 只能覆盖纯前端渲染路径,无法注入 fake LLM,也不能触发 Tauri IPC。
// 因此本文件仅作为占位,使用 `test.skip` 显式标记,避免在 CI 中误跑或
// 误判失败。不要在此文件引入额外的测试逻辑。
test.skip('Transform page loads and shows compare view', async () => {});
test.skip('Transform page flips between chapters', async () => {});
test.skip('Transform page selects different transformation versions', async () => {});