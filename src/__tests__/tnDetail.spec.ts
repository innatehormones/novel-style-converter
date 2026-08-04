import { describe, it, expect, vi, beforeEach } from 'vitest';
import { mount, flushPromises } from '@vue/test-utils';
import { createMemoryHistory, createRouter } from 'vue-router';
import { createPinia, setActivePinia } from 'pinia';
import TransformationNovelDetail from '../views/TransformationNovelDetail.vue';

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn().mockImplementation((cmd: string) => {
    if (cmd === 'list_transformation_chapters') return Promise.resolve([]);
    if (cmd === 'list_batches') return Promise.resolve([]);
    if (cmd === 'count_batches_by_status') return Promise.resolve({
      pending: 0, running: 0, paused: 0,
      completed: 0, terminated: 0, cancelled: 0,
    });
    return Promise.resolve(null);
  }),
}));

const router = createRouter({
  history: createMemoryHistory(),
  routes: [
    { path: '/library/transformation/:tnId', component: TransformationNovelDetail, props: true },
  ],
});

describe('TransformationNovelDetail', () => {
  beforeEach(async () => {
    setActivePinia(createPinia());
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
  });
});