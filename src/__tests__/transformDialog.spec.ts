// @vitest-environment happy-dom
import { describe, it, expect, beforeEach, vi } from 'vitest';
import { mount, flushPromises } from '@vue/test-utils';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));
import { invoke } from '@tauri-apps/api/core';
import TransformDialog from '../components/TransformDialog.vue';

const sampleTn = {
  id: 1, data_asset_id: 1, title: '热血版',
  created_at: '2026-07-26T00:00:00Z', chapters_count: 5,
};
const sampleModel = {
  id: 20, name: 'deepseek', base_url: 'http://x', api_key: 'k',
  model: 'deepseek-chat', max_tokens: null, temperature: null, concurrency: 3,
};

function mockListApis() {
  vi.mocked(invoke).mockImplementation((cmd: string) => {
    if (cmd === 'list_transformation_novels') return Promise.resolve([sampleTn]);
    if (cmd === 'list_models') return Promise.resolve([sampleModel]);
    if (cmd === 'enqueue_transformation_chapters') return Promise.resolve([100]);
    return Promise.reject(new Error(`unexpected: ${cmd}`));
  });
}

// Dialog 用 Teleport 把内容塞到 body,wrapper.find() 不会跨 teleport 查找;
// 与 cleaning-dialog.spec.ts 同 pattern:document.body.querySelector(...) 直查。
function q(sel: string): HTMLElement {
  const el = document.body.querySelector(sel);
  if (!el) throw new Error(`not found: ${sel}`);
  return el as HTMLElement;
}

describe('TransformDialog', () => {
  beforeEach(() => {
    vi.mocked(invoke).mockReset();
    mockListApis();
  });

  it('打开时并发拉 tn / model 列表', async () => {
    const wrapper = mount(TransformDialog, {
      props: {
        open: true, dataAssetId: 1, chapterId: 7,
        defaultPromptId: 10, defaultModelConfigId: 20,
      },
      attachTo: document.body,
    });
    await flushPromises();
    expect(invoke).toHaveBeenCalledWith('list_transformation_novels', { dataAssetId: 1 });
    expect(invoke).toHaveBeenCalledWith('list_models');
    wrapper.unmount();
  });

  it('提交时调 enqueue_transformation_chapters(prompt_id 来自 default)', async () => {
    const wrapper = mount(TransformDialog, {
      props: {
        open: true, dataAssetId: 1, chapterId: 7,
        defaultPromptId: 10, defaultModelConfigId: 20,
      },
      attachTo: document.body,
    });
    await flushPromises();
    (q('.tn-select') as HTMLSelectElement).value = '1';
    q('.tn-select').dispatchEvent(new Event('change', { bubbles: true }));
    (q('.model-select') as HTMLSelectElement).value = '20';
    q('.model-select').dispatchEvent(new Event('change', { bubbles: true }));
    (q('.ctx-prev-original') as HTMLInputElement).value = '0';
    q('.ctx-prev-original').dispatchEvent(new Event('input', { bubbles: true }));
    (q('.ctx-prev-transformed') as HTMLInputElement).value = '1';
    q('.ctx-prev-transformed').dispatchEvent(new Event('input', { bubbles: true }));
    (q('.ctx-next-original') as HTMLInputElement).value = '0';
    q('.ctx-next-original').dispatchEvent(new Event('input', { bubbles: true }));
    await flushPromises();
    q('.submit').click();
    await flushPromises();
    expect(invoke).toHaveBeenCalledWith('enqueue_transformation_chapters', {
      payload: {
        transformation_novel_id: 1,
        chapter_ids: [7],
        prompt_id: 10,
        model_config_id: 20,
        ctx_prev_original: 0,
        ctx_prev_transformed: 1,
        ctx_next_original: 0,
      },
    });
    wrapper.unmount();
  });

  it('提交失败时 inline 显示错误,不关闭 dialog', async () => {
    vi.mocked(invoke).mockImplementation((cmd: string) => {
      if (cmd === 'list_transformation_novels') return Promise.resolve([sampleTn]);
      if (cmd === 'list_models') return Promise.resolve([sampleModel]);
      if (cmd === 'enqueue_transformation_chapters') return Promise.reject(new Error('rate limit'));
      return Promise.reject(new Error(`unexpected: ${cmd}`));
    });
    const wrapper = mount(TransformDialog, {
      props: {
        open: true, dataAssetId: 1, chapterId: 7,
        defaultPromptId: 10, defaultModelConfigId: 20,
      },
      attachTo: document.body,
    });
    await flushPromises();
    (q('.tn-select') as HTMLSelectElement).value = '1';
    q('.tn-select').dispatchEvent(new Event('change', { bubbles: true }));
    (q('.model-select') as HTMLSelectElement).value = '20';
    q('.model-select').dispatchEvent(new Event('change', { bubbles: true }));
    await flushPromises();
    q('.submit').click();
    await flushPromises();
    expect(q('.error').textContent).toContain('rate limit');
    expect(wrapper.props('open')).toBe(true);
    wrapper.unmount();
  });

  it('无 defaultPromptId 时提交按钮禁用并提示', async () => {
    const wrapper = mount(TransformDialog, {
      props: { open: true, dataAssetId: 1, chapterId: 7 },
      attachTo: document.body,
    });
    await flushPromises();
    (q('.tn-select') as HTMLSelectElement).value = '1';
    q('.tn-select').dispatchEvent(new Event('change', { bubbles: true }));
    (q('.model-select') as HTMLSelectElement).value = '20';
    q('.model-select').dispatchEvent(new Event('change', { bubbles: true }));
    await flushPromises();
    const submit = q('.submit') as HTMLButtonElement;
    expect(submit.disabled).toBe(true);
    wrapper.unmount();
  });
});