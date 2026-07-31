# tests-e2e — Playwright 端到端

## 当前状态：占位为主

两个文件均处于「未真实运行」状态，原因见下方「为什么是占位」一节。

| 文件 | 内容 | 当前能否跑通 |
|---|---|---|
| `library.spec.ts` | 2 个前端渲染断言(空状态 + 弹窗校验) | **断言已过期** —— Library.vue 当前路由是 `/uploads`,标题是「上传文件」,没有「新增」按钮。两例都跑不过 |
| `transform.spec.ts` | 3 个 `test.skip()` 占位 | 全部跳过,无实际断言 |

> 后端集成路径由 `src/__tests__/` 下的 vitest store 测试覆盖(用 `vi.mock('@tauri-apps/api/core')` 隔离 IPC)。这部分是当前真正起作用的测试。

## 为什么是占位

Playwright 配置(`playwright.config.ts`)起的是 `pnpm run dev` 启动的 Vite dev server(http://localhost:43801)。这条路径有两个根本限制:

1. **不触发 Tauri IPC**。前端 `invoke(...)` 调用会被 mock 掉或直接报错,拿不到真实 Rust 后端响应。
2. **无法注入 fake LLM endpoint**。`JobQueue` 跑转换任务要打 `model_configs.base_url`,Playwright 起的纯 web 环境无介入点。

要写真正覆盖全链路的 e2e,需要补齐两件基础设施:

- **fake LLM endpoint**:可以是 `wiremock` 自起的 mock server(OpenAI 兼容 `/v1/chat/completions`),或本地起的 Ollama,或一段本地 HTTP handler,用于让 `DefaultTransformer.run_one` 跑通到 `Done`。
- **Tauri 运行时 harness**:`tauri-driver` / WebDriver 客户端、或 `@tauri-apps/cli` 的 headless 模式,使 Playwright 能连接到真实桌面窗口而非纯 Vite。

这两件就绪后,`transform.spec.ts` 即可改写为:
1. 启动 fake LLM + 设置 `ModelConfig.base_url` 指向它
2. 启动 Tauri 桌面应用
3. Playwright 通过 WebDriver 控制 webview,跑上传 → 解析 → 触发转换 → 等 Done → 进入 Transform 页 验证左右栏对照

## 跑 e2e

```bash
pnpm e2e
```

当前预期输出:两个 library 用例 fail(过期),三个 transform 用例 skip。无 panic / 无 hang 即视为通过。CI 上可考虑加 `--grep "transform" --grep-invert` 或将过期用例也加 `test.skip` / `test.fixme`,等真实 harness 就绪后再启用。

## 已知过期

`library.spec.ts` 两例当前会失败 —— 是预期失败,不是 bug。修法是按 `src/views/Library.vue` 当前 UI 重写(标题文案 + 按钮名 + 路由),或在修 e2e harness 阶段一并处理。