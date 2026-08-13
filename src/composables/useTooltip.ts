import { nextTick, ref } from 'vue';

// 单一 anchor / text / 可见性,模块级 singleton。TooltipHost 和所有 Tooltip 触发器共享。
const text = ref('');
const visible = ref(false);
const x = ref(0);
const y = ref(0);

const mouseX = ref(0);
const mouseY = ref(0);
let pendingAnchor: HTMLElement | null = null;
let pendingText = '';

function clamp(v: number, min: number, max: number): number {
  if (v < min) return min;
  if (v > max) return max;
  return v;
}

async function place() {
  const tipEl = document.getElementById('app-tooltip-host');
  if (!tipEl || !pendingAnchor) return;
  // 等一拍:DOM 中的 text 还没刷新时,tipRect 是旧值,会算出错误位置
  await nextTick();
  // await 期间可能被 hide() 把 pendingAnchor 清掉,不能继续写状态
  if (!pendingAnchor) return;

  const tipRect = tipEl.getBoundingClientRect();
  // 期望:tooltip 水平居中于鼠标,底部距鼠标 10px(鼠标上方)
  const desiredX = mouseX.value;
  const desiredY = mouseY.value - 10 - tipRect.height;

  const margin = 8;
  const maxX = window.innerWidth - margin - tipRect.width;
  const maxY = window.innerHeight - margin - tipRect.height;

  x.value = clamp(desiredX, margin + tipRect.width / 2, maxX + tipRect.width / 2);
  y.value = clamp(desiredY, margin, maxY);

  visible.value = true;
}

if (typeof window !== 'undefined') {
  // tooltip 跟随鼠标:每次 mousemove 都重算位置
  window.addEventListener('mousemove', (e) => {
    mouseX.value = e.clientX;
    mouseY.value = e.clientY;
    if (visible.value) void place();
  });
  // 滚动 / 窗口尺寸变化时也要重新定位(否则 tooltip 会停在原位)
  window.addEventListener('scroll', () => {
    if (visible.value) void place();
  }, true);
  window.addEventListener('resize', () => {
    if (visible.value) void place();
  });
}

export function useTooltip() {
  function show(t: string, anchor: HTMLElement) {
    pendingText = t;
    pendingAnchor = anchor;
    text.value = t;
    // 立即定位:不靠 enterTimer 延迟,让 tooltip 直接出现在鼠标处
    void place();
  }

  function hide() {
    // 立即清掉 anchor / text,任何在飞的 scroll/resize 都会被 ignore
    pendingAnchor = null;
    pendingText = '';
    visible.value = false;
    x.value = 0;
    y.value = 0;
  }

  return { text, visible, x, y, show, hide };
}

export type TooltipApi = ReturnType<typeof useTooltip>;
