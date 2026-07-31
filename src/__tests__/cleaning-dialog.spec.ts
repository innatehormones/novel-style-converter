// @vitest-environment happy-dom
import { describe, it, expect, beforeEach, vi, afterEach } from 'vitest';
import { mount, flushPromises } from '@vue/test-utils';
import CleaningDialog from '../components/CleaningDialog.vue';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));
import { invoke } from '@tauri-apps/api/core';

const basePreview = { cleaned_text: '　　A\n', lines_delta: 0, chars_delta: 2 };

beforeEach(() => {
  vi.mocked(invoke).mockReset();
});
afterEach(() => vi.useRealTimers());

describe('CleaningDialog', () => {
  it('renders 4 checked rules and triggers preview when opened', async () => {
    vi.mocked(invoke).mockResolvedValue(basePreview);
    const wrapper = mount(CleaningDialog, {
      props: { sourceText: 'A\n', open: true },
      attachTo: document.body,
    });
    await flushPromises();
    expect(document.body.querySelectorAll('input[type=checkbox]')).toHaveLength(4);
    expect(invoke).toHaveBeenCalledWith('preview_cleaning', expect.objectContaining({ text: 'A\n' }));
    wrapper.unmount();
  });

  it('confirm button is disabled when preview is a no-op', async () => {
    vi.mocked(invoke).mockResolvedValue({ cleaned_text: 'A\n', lines_delta: 0, chars_delta: 0 });
    const wrapper = mount(CleaningDialog, {
      props: { sourceText: 'A\n', open: true },
      attachTo: document.body,
    });
    await flushPromises();
    const buttons = document.body.querySelectorAll('button');
    const confirmBtn = Array.from(buttons).find((b) => (b.textContent ?? '').includes('确认回填'));
    expect(confirmBtn).toBeTruthy();
    expect((confirmBtn as HTMLButtonElement).disabled).toBe(true);
    wrapper.unmount();
  });

  it('clicking 确认回填 emits confirm with cleaned text', async () => {
    vi.mocked(invoke).mockResolvedValue(basePreview);
    const wrapper = mount(CleaningDialog, {
      props: { sourceText: 'A\n', open: true },
      attachTo: document.body,
    });
    await flushPromises();
    const confirmBtn = Array.from(document.body.querySelectorAll('button')).find((b) => (b.textContent ?? '').includes('确认回填'))!;
    expect(confirmBtn).toBeTruthy();
    confirmBtn.click();
    await flushPromises();
    const emitted = wrapper.emitted('confirm');
    expect(emitted).toBeTruthy();
    expect(emitted![0][0]).toBe('　　A\n');
    wrapper.unmount();
  });

  it('clicking 取消 closes without emitting confirm', async () => {
    vi.mocked(invoke).mockResolvedValue(basePreview);
    const wrapper = mount(CleaningDialog, {
      props: { sourceText: 'A\n', open: true },
      attachTo: document.body,
    });
    await flushPromises();
    const cancelBtn = Array.from(document.body.querySelectorAll('button')).find((b) => (b.textContent ?? '').includes('取消'))!;
    expect(cancelBtn).toBeTruthy();
    cancelBtn.click();
    await flushPromises();
    expect(wrapper.emitted('confirm')).toBeUndefined();
    expect(wrapper.emitted('update:open')).toBeTruthy();
    wrapper.unmount();
  });

  it('toggling a rule re-runs preview (after debounce)', async () => {
    vi.useFakeTimers();
    vi.mocked(invoke).mockResolvedValue(basePreview);
    const wrapper = mount(CleaningDialog, {
      props: { sourceText: 'A\n', open: true },
      attachTo: document.body,
    });
    await flushPromises();
    const before = vi.mocked(invoke).mock.calls.length;
    // 直接调用组件方法 toggleRule,跟用户在 checkbox 上点击后,Vue 内部调用的事件处理是同一条路径。
    // (Teleport → wrapper.findAll 找不到 checkbox,DOM 事件在 happy-dom 下也不可靠。)
    const vm = wrapper.vm as unknown as { toggleRule: (id: string, checked: boolean) => void };
    vm.toggleRule('add_indent_to_unindented', false);
    // Vue default flush='post' → 让 watcher 在 microtask 里跑,跑完再 advance timer 触发 runPreview。
    await flushPromises();
    await vi.advanceTimersByTimeAsync(200);
    await flushPromises();
    expect(vi.mocked(invoke).mock.calls.length).toBeGreaterThan(before);
    wrapper.unmount();
  });

  it('shows error banner when previewCleaning rejects', async () => {
    vi.mocked(invoke).mockRejectedValueOnce(new Error('boom'));
    const wrapper = mount(CleaningDialog, {
      props: { sourceText: 'A\n', open: true },
      attachTo: document.body,
    });
    await flushPromises();
    expect(document.body.textContent ?? '').toContain('boom');
    wrapper.unmount();
  });
});
