// M03-UI-08：测试专用环境声明——vitest 在 node 环境运行，允许读取仓库内
// 已提交的样式源做结构守卫；生产代码不使用 fs，也不引入 @types/node。
declare module 'fs' {
  export function readFileSync(path: string, encoding?: string): string;
}
