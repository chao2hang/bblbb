// SvelteKit 全局类型（app.d.ts）+ 测试用模块声明。

declare global {
  // eslint-disable-next-line @typescript-eslint/no-namespace
  namespace App {
    // 预留：Session 安全投影等类型将在会话实现时补充。
  }
}

// 允许在测试中直接引用组件源码（vitest/build 时由 Vite 的 ?raw 支持）。
declare module '*.svelte?raw' {
  const content: string;
  export default content;
}

export {};