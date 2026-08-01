import { setActivePinia, createPinia } from 'pinia';
import { describe, it, expect, beforeEach, vi } from 'vitest';

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(),
}));

import { invoke } from '@tauri-apps/api/core';
import { useLibraryStore } from '../stores/library';
import type { UploadSummary, TransformationNovelSummary, DataAssetRow } from '../ipc/types';

const sampleUpload: UploadSummary = {
  id: 1,
  sha256: 'x',
  filename: 'A.txt',
  byte_size: 100,
  uploaded_at: '2026-07-26T00:00:00Z',
  file_path: '/x',
};

const sampleTn: TransformationNovelSummary = {
  id: 10,
  data_asset_id: 1,
  title: 'A_热血版',
  created_at: '2026-07-26T00:00:00Z',
  chapters_count: 0,
};

const sampleDa: DataAssetRow = {
  id: 1,
  upload_id: 1,
  title: 'A',
  parsed_at: '2026-07-26T00:00:00Z',
  locked_at: null,
  filename: 'A.txt',
  byte_size: 100,
};

describe('library store', () => {
  beforeEach(() => {
    setActivePinia(createPinia());
    vi.mocked(invoke).mockReset();
  });

  it('load 并发取 uploads + data_assets + transformation_novels', async () => {
    vi.mocked(invoke).mockImplementation((cmd: string) => {
      if (cmd === 'list_uploads') return Promise.resolve([sampleUpload]);
      if (cmd === 'list_data_assets') return Promise.resolve([sampleDa]);
      if (cmd === 'list_transformation_novels') return Promise.resolve([sampleTn]);
      return Promise.reject(new Error(`unexpected cmd: ${cmd}`));
    });
    const store = useLibraryStore();
    await store.load();
    expect(store.uploads).toEqual([sampleUpload]);
    expect(store.dataAssets).toEqual([sampleDa]);
    expect(store.transformationNovels).toEqual([sampleTn]);
    expect(store.loading).toBe(false);
  });

  it('upload 调 upload_file 并 reload', async () => {
    vi.mocked(invoke).mockImplementation((cmd: string) => {
      if (cmd === 'upload_file') return Promise.resolve({ ...sampleUpload, id: 7 });
      if (cmd === 'list_uploads') return Promise.resolve([{ ...sampleUpload, id: 7 }]);
      if (cmd === 'list_data_assets') return Promise.resolve([]);
      if (cmd === 'list_transformation_novels') return Promise.resolve([]);
      return Promise.reject(new Error(`unexpected cmd: ${cmd}`));
    });
    const store = useLibraryStore();
    const result = await store.upload({ file_path: '/tmp/A.txt', filename: 'A.txt' });
    expect(invoke).toHaveBeenCalledWith('upload_file', { payload: { file_path: '/tmp/A.txt', filename: 'A.txt' } });
    expect(result.id).toBe(7);
    expect(store.uploads[0].id).toBe(7);
    expect(store.uploading).toBe(false);
  });

  it('upload 期间 uploading=true,失败后回落 false', async () => {
    let resolveUpload!: (v: unknown) => void;
    vi.mocked(invoke).mockImplementation((cmd: string) => {
      if (cmd === 'upload_file') return new Promise((res) => { resolveUpload = res; });
      if (cmd === 'list_uploads') return Promise.resolve([]);
      if (cmd === 'list_data_assets') return Promise.resolve([]);
      if (cmd === 'list_transformation_novels') return Promise.resolve([]);
      return Promise.reject(new Error(`unexpected cmd: ${cmd}`));
    });
    const store = useLibraryStore();
    const pending = store.upload({ file_path: '/tmp/big.txt', filename: 'big.txt' });
    expect(store.uploading).toBe(true);
    resolveUpload({ ...sampleUpload, id: 8 });
    await pending;
    expect(store.uploading).toBe(false);

    vi.mocked(invoke).mockImplementationOnce((cmd: string) => {
      if (cmd === 'upload_file') return Promise.reject(new Error('boom'));
      return Promise.resolve([]);
    });
    await expect(
      store.upload({ file_path: '/tmp/broken.txt', filename: 'broken.txt' }),
    ).rejects.toThrow('boom');
    expect(store.uploading).toBe(false);
  });

  it('removeUpload 发 delete_upload + 全文 reload(CASCADE 影响其他表)', async () => {
    let deleteUploadCalled = false;
    vi.mocked(invoke).mockImplementation((cmd: string) => {
      if (cmd === 'delete_upload') {
        deleteUploadCalled = true;
        return Promise.resolve(undefined);
      }
      if (cmd === 'list_uploads') return Promise.resolve([sampleUpload, { ...sampleUpload, id: 2 }]);
      if (cmd === 'list_data_assets') return Promise.resolve([]);
      if (cmd === 'list_transformation_novels') return Promise.resolve([]);
      return Promise.resolve(undefined);
    });
    const store = useLibraryStore();
    await store.load();
    await store.removeUpload(1);
    expect(deleteUploadCalled).toBe(true);
    expect(invoke).toHaveBeenCalledWith('delete_upload', { id: 1 });
    // load() 顺带重拉 uploads / data_assets / transformation_novels,
    // 删除 upload 后 FK CASCADE 会清 data_assets / transformation_novels,这里通过 load 同步。
    expect(invoke).toHaveBeenCalledWith('list_uploads');
    expect(invoke).toHaveBeenCalledWith('list_data_assets');
    expect(invoke).toHaveBeenCalledWith('list_transformation_novels', { dataAssetId: undefined });
  });

  it('createTransformationNovel 调 create_transformation_novel 并 reload', async () => {
    vi.mocked(invoke).mockImplementation((cmd: string) => {
      if (cmd === 'create_transformation_novel') return Promise.resolve(42);
      if (cmd === 'list_uploads') return Promise.resolve([]);
      if (cmd === 'list_data_assets') return Promise.resolve([]);
      if (cmd === 'list_transformation_novels') return Promise.resolve([{ ...sampleTn, id: 42 }]);
      return Promise.reject(new Error(`unexpected cmd: ${cmd}`));
    });
    const store = useLibraryStore();
    const id = await store.createTransformationNovel({ data_asset_id: 1, title: 'X' });
    expect(id).toBe(42);
    expect(invoke).toHaveBeenCalledWith('create_transformation_novel', { payload: { data_asset_id: 1, title: 'X' } });
    expect(store.transformationNovels[0].id).toBe(42);
  });

  it('renameTransformationNovel 发 update_transformation_novel', async () => {
    vi.mocked(invoke).mockImplementation((cmd: string) => {
      if (cmd === 'list_uploads') return Promise.resolve([]);
      if (cmd === 'list_data_assets') return Promise.resolve([]);
      if (cmd === 'list_transformation_novels') return Promise.resolve([{ ...sampleTn, title: 'NEW' }]);
      return Promise.resolve(undefined);
    });
    const store = useLibraryStore();
    await store.renameTransformationNovel({ id: 10, title: 'NEW' });
    expect(invoke).toHaveBeenCalledWith('update_transformation_novel', { payload: { id: 10, title: 'NEW' } });
    expect(store.transformationNovels[0].title).toBe('NEW');
  });

  it('removeTransformationNovel 过滤本地', async () => {
    vi.mocked(invoke).mockImplementation((cmd: string) => {
      if (cmd === 'list_uploads') return Promise.resolve([]);
      if (cmd === 'list_data_assets') return Promise.resolve([]);
      if (cmd === 'list_transformation_novels') return Promise.resolve([sampleTn]);
      return Promise.resolve(undefined);
    });
    const store = useLibraryStore();
    await store.load();
    await store.removeTransformationNovel(10);
    expect(invoke).toHaveBeenLastCalledWith('delete_transformation_novel', { id: 10 });
    expect(store.transformationNovels).toHaveLength(0);
  });

  it('removeDataAsset 发 delete_data_asset + 全文 reload(CASCADE 影响 transformation_novels)', async () => {
    const da2 = { ...sampleDa, id: 2, title: 'B' };
    let deleteDataAssetCalled = false;
    vi.mocked(invoke).mockImplementation((cmd: string) => {
      if (cmd === 'delete_data_asset') {
        deleteDataAssetCalled = true;
        return Promise.resolve(undefined);
      }
      if (cmd === 'list_uploads') return Promise.resolve([]);
      if (cmd === 'list_data_assets') return Promise.resolve([sampleDa, da2]);
      if (cmd === 'list_transformation_novels') return Promise.resolve([]);
      return Promise.resolve(undefined);
    });
    const store = useLibraryStore();
    await store.load();
    expect(store.dataAssets).toHaveLength(2);
    await store.removeDataAsset(1);
    expect(deleteDataAssetCalled).toBe(true);
    expect(invoke).toHaveBeenCalledWith('delete_data_asset', { dataAssetId: 1 });
    expect(invoke).toHaveBeenCalledWith('list_transformation_novels', { dataAssetId: undefined });
  });

  it('捕获错误字符串', async () => {
    vi.mocked(invoke).mockRejectedValueOnce(new Error('boom'));
    const store = useLibraryStore();
    await store.load();
    expect(store.error).toBe('boom');
  });
});