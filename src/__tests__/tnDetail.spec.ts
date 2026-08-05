import { describe, it, expect, vi, beforeEach } from 'vitest';
import { mount, flushPromises } from '@vue/test-utils';
import { createMemoryHistory, createRouter } from 'vue-router';
import { createPinia, setActivePinia } from 'pinia';
import TransformationNovelDetail from '../views/TransformationNovelDetail.vue';

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn().mockImplementation((cmd: string) => {
    if (cmd === 'list_transformation_source_chapters') return Promise.resolve([]);
    if (cmd === 'list_workflows') return Promise.resolve([]);
    return Promise.resolve(null);
  }),
}));

import { invoke } from '@tauri-apps/api/core';

const router = createRouter({
  history: createMemoryHistory(),
  routes: [
    { path: '/library/transformation/:tnId', component: TransformationNovelDetail, props: true },
  ],
});

describe('TransformationNovelDetail', () => {
  beforeEach(async () => {
    setActivePinia(createPinia());
    vi.mocked(invoke).mockReset();
    vi.mocked(invoke).mockImplementation((cmd: string) => {
      if (cmd === 'list_transformation_source_chapters') return Promise.resolve([]);
      if (cmd === 'list_workflows') return Promise.resolve([]);
      return Promise.resolve(null);
    });
    await router.push('/library/transformation/42');
    await router.isReady();
  });

  it('mounts and shows chapters tab by default', async () => {
    const wrapper = mount(TransformationNovelDetail, {
      props: { tnId: 42 },
      global: { plugins: [router] },
    });
    await flushPromises();
    expect(wrapper.text()).toContain('TN #42');
    expect(wrapper.text()).toContain('章节一览');
    expect(wrapper.text()).toContain('工作流');
  });
});
