// ============================================================
// 客户业务模块（交付约定：ES Module，导出业务函数）
// 该文件经 CLI 加密后交付为 PNG，客户侧由 runtime-sdk 解密并 import。
// ============================================================

/** 业务函数：向客户打招呼 */
export function greet(name) {
  const msg = `你好，${name}！业务代码已通过 js2png 提前解密并常驻内存，调用零等待。`;
  console.log(msg);
  return msg;
}

/** 业务数据 */
export const version = '1.0.0';

export default { greet, version };
