// @vitest-environment happy-dom
import { describe, it, expect, beforeEach, vi } from 'vitest';
import { mount, flushPromises } from '@vue/test-utils';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));
import { invoke } from '@tauri-apps/api/core';
import {
  createTransformationNovel,
  updateTransformationNovel,
} from '../ipc/commands';
import TransformationNovelDialog from '../components/TransformationNovelDialog.vue';

function q(sel: string): HTMLElement {
  const el = document.body.querySelector(sel);
  if (!el) throw new Error(`not found: ${sel}`);
  return el as HTMLElement;
}

describe('TransformationNovelDialog', () => {
  beforeEach(() => {
    vi.mocked(invoke).mockReset();
  });

  it('默认未填字段时 emit 包含 null 值', async () => {
    const wrapper = mount(TransformationNovelDialog, {
      props: { open: true, dataAssetId: 5 },
      attachTo: document.body,
    });
    await flushPromises();
    (q('.title-input') as HTMLInputElement).value = '热血版';
    q('.title-input').dispatchEvent(new Event('input', { bubbles: true }));
    await flushPromises();
    q('.submit').click();
    await flushPromises();
    const emitted = wrapper.emitted('submit');
    expect(emitted).toBeTruthy();
    expect(emitted).toHaveLength(1);
    expect(emitted![0][0]).toEqual({
      data_asset_id: 5,
      title: '热血版',
      default_model_config_id: null,
      default_prompt_id: null,
      default_mode: null,
    });
    wrapper.unmount();
  });

  it('设置三个默认字段后 emit 携带对应值', async () => {
    const wrapper = mount(TransformationNovelDialog, {
      props: { open: true, dataAssetId: 5 },
      attachTo: document.body,
    });
    await flushPromises();
    (q('.title-input') as HTMLInputElement).value = '斗破_热血版';
    q('.title-input').dispatchEvent(new Event('input', { bubbles: true }));
    const modelInput = q('.default-model-input') as HTMLInputElement;
    modelInput.value = '3';
    modelInput.dispatchEvent(new Event('input', { bubbles: true }));
    const promptInput = q('.default-prompt-input') as HTMLInputElement;
    promptInput.value = '7';
    promptInput.dispatchEvent(new Event('input', { bubbles: true }));
    (q('.default-mode-select') as HTMLSelectElement).value = 'style';
    q('.default-mode-select').dispatchEvent(new Event('change', { bubbles: true }));
    await flushPromises();
    q('.submit').click();
    await flushPromises();
    const emitted = wrapper.emitted('submit');
    expect(emitted).toBeTruthy();
    expect(emitted).toHaveLength(1);
    expect(emitted![0][0]).toEqual({
      data_asset_id: 5,
      title: '斗破_热血版',
      default_model_config_id: 3,
      default_prompt_id: 7,
      default_mode: 'style',
    });
    wrapper.unmount();
  });

  it('open 再次触发时表单复位为 null', async () => {
    const wrapper = mount(TransformationNovelDialog, {
      props: { open: true, dataAssetId: 5 },
      attachTo: document.body,
    });
    await flushPromises();
    // 填值
    (q('.title-input') as HTMLInputElement).value = 'X';
    q('.title-input').dispatchEvent(new Event('input', { bubbles: true }));
    const modelInput = q('.default-model-input') as HTMLInputElement;
    modelInput.value = '9';
    modelInput.dispatchEvent(new Event('input', { bubbles: true }));
    (q('.default-mode-select') as HTMLSelectElement).value = 'compress';
    q('.default-mode-select').dispatchEvent(new Event('change', { bubbles: true }));
    await flushPromises();
    // 关闭 → 重开
    await wrapper.setProps({ open: false });
    await flushPromises();
    await wrapper.setProps({ open: true });
    await flushPromises();
    // 表单应被复位
    expect((q('.title-input') as HTMLInputElement).value).toBe('');
    expect((q('.default-model-input') as HTMLInputElement).value).toBe('');
    expect((q('.default-prompt-input') as HTMLInputElement).value).toBe('');
    expect((q('.default-mode-select') as HTMLSelectElement).value).toBe('');
    wrapper.unmount();
  });

  it('标题为空时 submit 按钮禁用', async () => {
    const wrapper = mount(TransformationNovelDialog, {
      props: { open: true, dataAssetId: 5 },
      attachTo: document.body,
    });
    await flushPromises();
    const submit = q('.submit') as HTMLButtonElement;
    expect(submit.disabled).toBe(true);
    wrapper.unmount();
  });
});

describe('createTransformationNovel / updateTransformationNovel wrappers', () => {
  beforeEach(() => {
    vi.mocked(invoke).mockReset();
  });

  it('createTransformationNovel 把内层字段原样透传', async () => {
    vi.mocked(invoke).mockResolvedValueOnce(7);
    const id = await createTransformationNovel({
      data_asset_id: 5,
      title: 'X',
      default_model_config_id: 3,
      default_prompt_id: 2,
      default_mode: 'compress',
    });
    expect(id).toBe(7);
    expect(invoke).toHaveBeenCalledWith('create_transformation_novel', {
      payload: {
        data_asset_id: 5,
        title: 'X',
        default_model_config_id: 3,
        default_prompt_id: 2,
        default_mode: 'compress',
      },
    });
  });

  it('createTransformationNovel 支持 null 默认字段', async () => {
    vi.mocked(invoke).mockResolvedValueOnce(8);
    const id = await createTransformationNovel({
      data_asset_id: 5,
      title: 'Y',
      default_model_config_id: null,
      default_prompt_id: null,
      default_mode: null,
    });
    expect(id).toBe(8);
    expect(invoke).toHaveBeenCalledWith('create_transformation_novel', {
      payload: {
        data_asset_id: 5,
        title: 'Y',
        default_model_config_id: null,
        default_prompt_id: null,
        default_mode: null,
      },
    });
  });

  it('updateTransformationNovel 把内层字段原样透传', async () => {
    vi.mocked(invoke).mockResolvedValueOnce(undefined);
    await updateTransformationNovel({
      id: 7,
      title: 'NEW',
      default_model_config_id: 3,
      default_prompt_id: 2,
      default_mode: 'style',
    });
    expect(invoke).toHaveBeenCalledWith('update_transformation_novel', {
      payload: {
        id: 7,
        title: 'NEW',
        default_model_config_id: 3,
        default_prompt_id: 2,
        default_mode: 'style',
      },
    });
  });
});
