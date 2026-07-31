import { createRouter, createWebHistory } from 'vue-router';
import Models from '../views/Models.vue';
import Library from '../views/Library.vue';
import Upload from '../views/Upload.vue';
import DataAsset from '../views/DataAsset.vue';
import ParseWizard from '../views/parse.vue';
import Transform from '../views/Transform.vue';
import { findDataAssetByUpload } from '../ipc/commands';

const router = createRouter({
  history: createWebHistory(),
  routes: [
    { path: '/', redirect: '/uploads' },
    { path: '/library', redirect: '/uploads' },
    { path: '/uploads', component: Library, meta: { libraryPage: 'uploads' } },
    { path: '/data-assets', component: Library, meta: { libraryPage: 'data-assets' } },
    { path: '/transformations', component: Library, meta: { libraryPage: 'transformations' } },
    {
      path: '/library/upload/:uploadId',
      component: Upload,
      name: 'upload',
    },
    {
      path: '/library/upload/:uploadId/parse',
      component: ParseWizard,
      name: 'parse-wizard',
    },
    {
      path: '/library/data/:dataAssetId',
      component: DataAsset,
      name: 'data-asset',
    },
    {
      path: '/library/transform/:chapterId',
      component: Transform,
      name: 'transform',
    },
    { path: '/models', component: Models },
  ],
});

/// 旧路由 `/library/:uploadId/clean` `/library/:uploadId/chapters` `/library/:uploadId/preview` 重定向。
/// 以及新路由 `/library/upload/:uploadId/parse` 的 guard:
/// 若该 upload 已有 data_asset → 跳到 DataAsset 页;否则跳到 Upload 页 / 放行。
/// 已有 data_asset 重新解析要走 DataAsset.vue 的 delete_data_asset 路径,
/// 不能再从 parse wizard 直接 commit(后端 guard 会拒绝,前端文案误导)。
router.beforeEach(async (to) => {
  const oldMatch = to.path.match(/^\/library\/(\d+)\/(clean|chapters|preview)$/);
  if (oldMatch) {
    const uploadId = Number(oldMatch[1]);
    const daId = await findDataAssetByUpload(uploadId);
    return daId != null ? `/library/data/${daId}` : `/library/upload/${uploadId}`;
  }
  const parseMatch = to.path.match(/^\/library\/upload\/(\d+)\/parse$/);
  if (parseMatch) {
    const uploadId = Number(parseMatch[1]);
    const daId = await findDataAssetByUpload(uploadId);
    if (daId != null) return `/library/data/${daId}`;
  }
});

export default router;