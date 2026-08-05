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

  it('opens, shows 6 fields + selected count, submits snake_case CreateWorkflowInput', async () => {
    const wrapper = mount(CreateBatchDialog, {
      props: {
        tnId: 42,
        defaultPromptId: 1,
        defaultModelConfigId: 1,
        defaultMode: 'compress',
        selectedChapterIds: [10, 11],
        open: true,
      },
      attachTo: document.body,
    });
    await flushPromises();
    await flushPromises();

    expect(document.body.textContent ?? '').toContain('新建工作流');
    expect(document.body.textContent ?? '').toContain('已选');
    expect(document.body.textContent ?? '').toContain('2');
    expect(document.body.textContent ?? '').toContain('提示词模板');
    expect(document.body.textContent ?? '').toContain('模型配置');
    expect(document.body.textContent ?? '').not.toContain('失败策略');
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
    expect(payload).toEqual({
      tn_id: 42,
      label: null,
      chapter_ids: [10, 11],
      prompt_id: 1,
      model_config_id: 1,
      mode: 'compress',
      ctx_prev_original: 0,
      ctx_prev_transformed: 0,
      ctx_next_original: 0,
    });
  });

  it('infers mode from selected prompt kind even when TN defaultMode is null', async () => {
    const wrapper = mount(CreateBatchDialog, {
      props: {
        tnId: 1,
        defaultMode: null,
        selectedChapterIds: [7],
        open: true,
      },
      attachTo: document.body,
    });
    await flushPromises();
    await flushPromises();

    const vm = wrapper.vm as any;
    vm.promptId = 2;  // kind='style'
    vm.modelConfigId = 1;
    await vm.onSubmit();

    const submits = wrapper.emitted('submit');
    expect(submits).toBeTruthy();
    expect((submits![0] as any)[0].mode).toBe('style');
  });

  it('selectedChapterIds=[] 时 canSubmit=false', async () => {
    const wrapper = mount(CreateBatchDialog, {
      props: {
        tnId: 1,
        selectedChapterIds: [],
        defaultPromptId: 1,
        defaultModelConfigId: 1,
        open: true,
      },
      attachTo: document.body,
    });
    await flushPromises();
    await flushPromises();
    const vm = wrapper.vm as any;
    expect(vm.canSubmit).toBe(false);
  });
});
