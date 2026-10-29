// js2png-wasm 运行时 SDK 类型声明
export type Js2PngMode = 'module' | 'script';

export interface Js2PngLoadOptions {
  /** 业务代码形态：ES Module（默认）或挂载到 window 的普通脚本 */
  mode?: Js2PngMode;
}

export declare class Js2Png {
  constructor(api: Record<string, any>, source: string);

  /** 解密出的完整原始 JS 源码 */
  source: string;

  /** 解密模块的全部导出（module 模式）或 window（script 模式） */
  readonly exports: Record<string, any>;

  /** 调用解密模块中的函数，支持点路径：call('greet')、call('MyApp.greet') */
  call<T = unknown>(path: string, ...args: unknown[]): T;
}

/** 加载并解密 PNG，返回 Js2Png 实例（按 pngUrl + key + mode 幂等缓存） */
export declare function load(
  pngUrl: string,
  key?: string | null,
  opts?: Js2PngLoadOptions
): Promise<Js2Png>;

/** 预加载：页面启动即开始解密，返回与 load() 相同的 Promise（失败时打印错误但不抛出未捕获异常） */
export declare function preload(
  pngUrl: string,
  key?: string | null,
  opts?: Js2PngLoadOptions
): Promise<Js2Png>;
