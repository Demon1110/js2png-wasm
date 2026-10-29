// ============================================================
// 示例业务代码：js2png 加密交付演示
// 该文件将通过 js2png-cli 加密嵌入 PNG，
// 交付端在浏览器中由 WASM 解密还原后执行。
// ============================================================

/**
 * 生成一段问候语
 * @param {string} name 姓名
 * @returns {string} 问候文本
 */
function greet(name) {
  const msg = `你好，${name}！`
    + '\n这段代码已通过 js2png 加密嵌入 PNG 图片，'
    + '并在浏览器中由 WASM 解密还原后执行。';
  console.log(msg);
  return msg;
}

/** 斐波那契数列（演示业务逻辑） */
function fib(n) {
  if (n <= 1) return n;
  return fib(n - 1) + fib(n - 2);
}

// 业务入口
greet('World');
console.log('fib(10) =', fib(10));
