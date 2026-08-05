// M03-UI-08/09：测试专用环境声明——vitest 在 node 环境运行，允许读取仓库内
// 文件做结构守卫；生产代码不使用 fs，也不引入 @types/node。
//
// 仅声明本测试需要的子集；withFileTypes:true 时返回 Dirent（带 isDirectory()），
// 其余形态返回 string[]。测试文件通过 `/// <reference path>` 引用本文件。
declare module 'fs' {
  export interface Dirent {
    name: string;
    isDirectory(): boolean;
  }
  export function readFileSync(path: string, encoding?: string): string;
  export function readdirSync(
    path: string,
    options?: { encoding?: string | null; withFileTypes?: false }
  ): string[];
  export function readdirSync(
    path: string,
    options: { encoding?: string | null; withFileTypes: true }
  ): Dirent[];
  export function existsSync(path: string): boolean;
}
