# 工作流转正数据资产设计

**日期**：2026-08-12  
**状态**：已完成 brainstorming，待用户审阅书面 spec  
**替代范围**：补 `2026-08-04-workflow-results-design.md` §2.2/§3.3/§9.3 中"结果集转正不在本次范围"那一句话，把转正做成完整可用的功能。

## 1. 目标

把"工作流 Stopped 后的产物"显式落到数据资产层。一个 workflow 跑完（含部分失败）后，用户主动选择把它的"成功用转换、失败用原文"的合成结果派生为一个新的数据资产，从此该派生资产在工作流层之外独立存在、可单独使用、可单独删除。

工作流与数据资产继续是两层独立实体：
- 工作流层负责"一次转换执行的过程与结果集"；
- 数据资产层负责"可独立使用的章节集合"。

派生是单向、append-only 的，不会反过来改工作流。

## 2. 范围

### 2.1 包含

> 约定：本 spec 范围内的所有改动都是允许破坏性的（测试阶段可清库重建），不做老数据兼容层。

- `data_assets` 表扩展 `kind` / `source_workflow_id` / `source_data_asset_id` / `note` 字段（migration 0021）；
- `promote_workflow(batch_id, title)` 单事务从工作流派生新数据资产；
- 二元填充规则：done → wrc.content；failed/skipped → 源 chapter.body；`source_kind` 标注来源；
- 数据资产层 UI 区分"源 / 派生"行，派生行展示溯源到工作流 + 计数；
- 工作流详情页加"转为数据资产"按钮 + 弹窗 + 列表行 `已转正 × N` tag；
- 上传页显示该上传派生 N 个数据资产；
- 章节解析页支持查看 promoted da 的章节（只读，body 直接展示）；
- 允许同一 workflow 多次派生；
- 删 source da 时 promoted da 保留但 `source_data_asset_id` SET NULL（保留独立性，断开溯源）；
- 删 workflow 时 promoted da 保留但 `source_workflow_id` SET NULL。

### 2.2 不包含

- 跨工作流的结果合并；
- promoted da 的二次转正链路（暂时不阻止，但不重点支持）；
- 转正内容的可编辑（promoted da 的章节是只读）；
- 转正产物的版本号 / diff（仅按时间排序）；
- 自动转正（始终用户手动触发）。

## 3. 核心业务约束

### 3.1 数据资产的双语义

`data_assets.kind` ∈ {`source`, `promoted`}：
- `source` —— 由 upload 经 splitter 解析得到，`source_workflow_id` / `source_data_asset_id` 为 NULL。
- `promoted` —— 由 workflow 转正得到，`source_workflow_id` 指向 batch_id，`source_data_asset_id` 指向本次转正所基于的源 da（始终是 split 出来的 kind=source 的 da，不链式引用其他 promoted），`upload_id` 沿用源 da 的 upload_id。

两种 da 在数据库层是平级表行，在 UI 上以 `类型` 列区分。

### 3.2 独立性原则

- 转正出的 promoted da 与原 workflow 没有强依赖：`workflow_id` SET NULL。
- promoted da 与源 da 没有强依赖：`source_data_asset_id` SET NULL。
- promoted da 删 chapter 时仅删除自己的章节，不影响源 da 的章节。
- 删源 upload 时不影响 promoted da（沿用 upload_id 是软引用 metadata）。

### 3.3 填充规则

对 workflow 内每一个已选 chapter（`tc` 指 `transformation_chapters`，`wrc` 指 `workflow_result_chapters`）：

| transformation_chapters.status | source_kind | new chapter.body |
|---|---|---|
| `done` | `'transformed'` | `wrc.content`（转换后文本） |
| `failed` 或 `skipped` | `'original'` | 源 `chapter.body`（原文） |

结果集空槽（failed/skipped）允许转正，符合 spec §3.3 已有的设计意图。

### 3.4 重复转正

允许同一 workflow 多次派生。每次独立事务创建新 da；不与历史派生冲突；UI 通过 `已转正 × N` tag 展示总数。

## 4. 数据模型

### 4.1 Migration 0021（破坏性扩展 data_assets）

```sql
ALTER TABLE data_assets ADD COLUMN kind TEXT NOT NULL DEFAULT 'source';
ALTER TABLE data_assets ADD COLUMN source_workflow_id INTEGER REFERENCES batches(id) ON DELETE SET NULL;
ALTER TABLE data_assets ADD COLUMN source_data_asset_id INTEGER REFERENCES data_assets(id) ON DELETE SET NULL;
ALTER TABLE data_assets ADD COLUMN note TEXT NOT NULL DEFAULT '';

CREATE INDEX idx_data_assets_kind ON data_assets(kind);
CREATE INDEX idx_data_assets_source_workflow ON data_assets(source_workflow_id);
```

- 现有 data_assets 自动继承 `kind='source'`（default），无需回填。
- 测试阶段如果迁移失败 → 直接删 `nsc.db` 重跑，不写回滚脚本。

### 4.2 chapters 表保持不变

v15 之后 `chapters.body` 已是独立副本；转正时为新 promoted da 插入 N 个新 chapter（body 已就绪）。

### 4.3 ON DELETE 矩阵

> 自引用 FK `data_assets.source_data_asset_id` 指向同一张表的另一行；SQLite 要求子行必须后插入，所以转正时**必须先确认源 da 已存在**（它必然存在，因为 workflow 是从它创建的），再 INSERT 新 promoted da。
|---|---|
| upload | promoted da.upload_id 不变（软引用），UI 不显示级联 |
| workflow（batch） | promoted da.source_workflow_id SET NULL |
| source data_asset | promoted da.source_data_asset_id SET NULL |
| promoted data_asset | 自身删除，chapters 级联删除 |
| chapter（被 promoted da 持有） | 仅删除该 promoted da 的章节 |

### 4.4 不变式（invariants）

- `tc.status='done'` 必须有 `wrc.content IS NOT NULL`；违反说明数据损坏，转正时报错。
- `batch.status='stopped'` 时所有 tc 必须 ∈ {`done`, `failed`, `skipped`}；违反报错。
- `promoted_data_asset.source_workflow_id IS NOT NULL`（用于 UI 跳转到 workflow 详情）。

## 5. 业务规则

### 5.1 `promote_workflow(batch_id, title)` 单事务

1. 读 batch；校验 `batch.status == 'stopped'`，否则 `Validation`。
2. 读 workflow_results 行；按 batch_id 拿 `result_id`。
3. 读所有 tc 行 JOIN wrc 行；遍历每个 chapter：
   - 校验 tc.status ∈ {`done`, `failed`, `skipped`}，否则 `Validation`。
   - 校验 `tc.status='done' → wrc.content IS NOT NULL`，否则 `Validation`（invariant）。
   - 计算新 chapter.body：
     - `tc.status='done'` → `wrc.content`，source_kind=`'transformed'`
     - 其他 → 源 `chapter.body`，source_kind=`'original'`
4. 读 source data_asset 拿 `upload_id`。
5. INSERT `data_assets(kind='promoted', source_workflow_id=batch_id, source_data_asset_id=原 da.id, upload_id=沿用源, title, parsed_at=now, note='')`。
6. INSERT N × `chapters(data_asset_id=new_da.id, idx, title, body, word_count, source_chapter_id=原 chapter.id, source_kind)`。
7. 任一 INSERT 失败 → ROLLBACK + 整事务回滚。
8. COMMIT；返回新 DataAsset。

### 5.2 错误分类

| 场景 | 错误 |
|---|---|
| batch 不存在 | `NotFound` |
| batch 状态非 stopped | `Validation("workflow 必须 Stopped 才能转正")` |
| tc 含 pending/running | `Validation("workflow 含未完成任务")` |
| tc.status='done' 但 wrc.content IS NULL | `Validation("数据损坏:done 章节缺内容")` |
| INSERT 失败 | 事务回滚，错误冒泡 |

所有错误经 IPC 层转 string → 前端弹错。不静默兜底。

### 5.3 派生计数

`promoted_count(workflow_id)` = `SELECT COUNT(*) FROM data_assets WHERE source_workflow_id = ?`。
`promoted_count(data_asset)` = `SELECT COUNT(*) FROM data_assets WHERE source_data_asset_id = ?`。

## 6. IPC 与查询边界

### 6.1 新增命令

- `promote_workflow(batch_id, title) -> DataAsset`（调 §5.1）
- `count_promoted_data_assets_by_workflow(batch_id) -> i64`
- `list_promoted_data_assets_for_workflow(batch_id) -> Vec<DataAsset>`
- `list_data_assets_by_upload(upload_id) -> Vec<DataAsset>`

### 6.2 修改命令

- `list_data_assets()` → `Vec<DataAssetWithUpload>`：每行加 `kind` + `source_workflow_id` + `promoted_count` 字段。
- `list_workflows(tn_id)` → `Vec<WorkflowSummary>`：每行加 `promoted_count`。
- `get_workflow(batch_id)` → `WorkflowSummary`：加 `promoted_count`。

### 6.3 不变命令

- `list_workflow_chapters(batch_id)` 不变。
- `create_workflow` / `stop_workflow` / `retry_workflow_chapters` 不变。

## 7. UI 改动

### 7.1 工作流详情页 `TransformationNovelDetail.vue`

- 工作流 tab 列表行右侧加 `已转正 × N` tag（`promoted_count > 0` 时显示）。
- 工作流详情弹窗顶部（Stopped 状态时）显示 `▶ 转为数据资产` 按钮。
- 点击 → 打开 `PromoteWorkflowDialog.vue`：
  - 标题输入框（必填，默认值 `{source_data_asset.title} - {workflow.label || '工作流 #' + batch_id}`）。
  - 摘要行：`X 章将使用转换结果，Y 章使用原文，Z 章被跳过用原文`。
  - 按钮：`取消` / `确认转正`。
  - 确认 → 调 `promote_workflow` → 成功后关闭弹窗 + 刷新 tag。

### 7.2 数据资产页 `Library.vue`

- 列表行加 `类型` 列：
  - `源`（kind=source）
  - `派生`（kind=promoted，带 `来自工作流 #X` 链接）
- 加 `派生数` 列（每个 da 被引用为 source 的次数）。
- 行点击跳章节解析页（按 kind 分流）。

### 7.3 上传页 `Uploads.vue`

- 列表行加 `派生 da 数` 列/标签（`COUNT(*)`）。
- 点击 → 跳过滤后的 Library（`upload_id = ?`）。

### 7.4 章节解析页（共享 `Parse.vue` / 章节读取组件）

- 接收 `data_asset_id`：
  - `kind=source` → 现有行为（左侧列表 + 右侧正文 + 标题编辑等）。
  - `kind=promoted` → 只读模式（标题不可编辑，无"重新解析"按钮）。
- 章节行显示 `类型` 列：`转换`（`source_kind='transformed'`）/ `原文`（`source_kind='original'`）。

### 7.5 模型/提示词页（不动）。

## 8. 错误处理

| 场景 | 行为 |
|---|---|
| 任何 §5.2 错误 | IPC 返回 string → 前端弹错 |
| 重复转正 | 成功，按已转正 tag 计数 |
| 删 source da | promoted da 保留，断开溯源 |
| 删 workflow | promoted da 保留，断开溯源 |
| 删 promoted da | 同步删章节 |
| 网络/序列化失败 | 沿用现有 IPC 错误链 |

## 9. 测试策略

### 9.1 Rust

- migration 0021 可重复执行；
- 现有 `data_assets` 默认 `kind='source'`；
- `create_promoted_from_workflow` 单事务原子性（任一 INSERT 失败 → ROLLBACK）；
- 前置校验：batch 非 Stopped → Validation；
- tc 含 pending/running → Validation；
- `tc.status='done'` 但 `wrc.content IS NULL` → Validation；
- 填充规则：done → wrc.content + transformed；failed/skipped → 源 body + original；
- 允许重复转正（连续 3 次产生 3 个独立 da）；
- ON DELETE 矩阵：删 source da → SET NULL；删 workflow → SET NULL；删 promoted da → 章节级联。

### 9.2 前端手工验证

1. 上传文件 → 解析章节 → 创建 TN；
2. 创建 workflow → 等待 Stopped；
3. 工作流详情点"转正" → 弹窗填 title → 确认；
4. Library 列表看到新 `派生` 行 + 跳章节解析页能读 body；
5. 工作流 tab 行显示 `已转正 × 1` tag；
6. 上传页行显示 `派生 1 个 da` tag；
7. 二次转正同 workflow → 产生第 2 个 promoted da；
8. 删 promoted da → Library 列表消失 + 工作流 tab 计数 -1。

## 10. 完成判据

- §1-§7 全部落地（migration + repo + IPC + UI）；
- 现有 22/22 测试仍 pass + 新增 §9.1 关键路径测试 pass；
- §9.2 手工验证黄金路径全通；
- `cargo check --workspace` + `npx vite build` 无 error/warning；
- 迁移 0021 在空 db 和已运行 db 上均可执行。