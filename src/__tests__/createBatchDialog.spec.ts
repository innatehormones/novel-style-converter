// @vitest-environment happy-dom
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { mount, flushPromises } from '@vue/test-utils';
import { createPinia, setActivePinia } from 'pinia';
import CreateBatchDialog from '../components/CreateBatchDialog.vue';

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn().mockImplementation((cmd: string) => {
    if (cmd === 'list_prompts') return Promise.resolve([
      { id: 1, name: '通用压缩', kind: 'compress', template: '', is_builtin: true },
      { id: 2, name: '文学风格', kind: 'style', template: '', is_builtin: true },
    ]);
    if (cmd === 'list_models') return Promise.resolve([
      { id: 1, name: 'gpt-4o-mini', base_url: 'x', api_key: 'k', model: 'gpt-4o-mini' },
    ]);
    return Promise.resolve(null);
  }),
}));

describe('CreateBatchDialog', () => {
  beforeEach(() => {
    setActivePinia(createPinia());
  });

  it('opens, shows 6 fields, then submits via emit', async () => {
    const wrapper = mount(CreateBatchDialog, {
      props: {
        tnId: 42,
        defaultPromptId: 1,
        defaultModelConfigId: 1,
        defaultMode: 'compress',
        open: true,
      },
      attachTo: document.body,
    });
    await flushPromises();
    await flushPromises();

    expect(document.body.textContent ?? '').toContain('新建工作流');
    expect(document.body.textContent ?? '').toContain('提示词模板');
    expect(document.body.textContent ?? '').toContain('模型配置');
    expect(document.body.textContent ?? '').toContain('失败策略');
    expect(document.body.textContent ?? '').toContain('前文原文');
    expect(document.body.textContent ?? '').toContain('前文转换');
    expect(document.body.textContent ?? '').toContain('后文原文');

    const vm = wrapper.vm as any;
    vm.promptId = 1;
    vm.modelConfigId = 1;
    await vm.onSubmit();

    const submits = wrapper.emitted('submit');
    expect(submits).toBeTruthy();
    expect(submits!.length).toBeGreaterThan(0);
    const payload = (submits![0] as any)[0];
    expect(payload.overrides.mode).toBe('compress');
    expect(payload.overrides.prompt_id).toBe(1);
    expect(payload.overrides.model_config_id).toBe(1);
    expect(payload.on_failure_policy).toBe('pause_and_review');
    expect(payload.label).toBeNull();
  });

  it('rejects submit when defaultMode is null', async () => {
    const wrapper = mount(CreateBatchDialog, {
      props: { tnId: 1, defaultMode: null, open: true },
      attachTo: document.body,
    });
    await flushPromises();
    await flushPromises();

    const vm = wrapper.vm as any;
    vm.promptId = 1;
    vm.modelConfigId = 1;
    await vm.onSubmit();

    expect(wrapper.emitted('submit')).toBeFalsy();
    expect(document.body.textContent ?? '').toContain('缺少默认 mode');
  });
});