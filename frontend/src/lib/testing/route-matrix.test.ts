// M03-UI-09：原型 → 生产路由矩阵结构守卫。
//
//  - shipped 路由的 +page.svelte（或 +error.svelte）必须存在于 src/routes；
//  - 生产源码绝不导入原型（prototype/）的 mock/store/脚本数据源
//    （M14-ROUTES-07：生产前端不依赖原型数据）；
//  - 矩阵覆盖原型 router.js 的全部静态/动态路由（设计回归完整性）。
import { describe, expect, it } from 'vitest';
import { readdirSync, readFileSync, existsSync } from 'fs';
import { PROTOTYPE_ROUTE_MATRIX } from '$lib/route-matrix';

/** 从磁盘枚举生产 +page.svelte 路由（SvelteKit [param] 归一化为 {param}）。 */
function productionRoutes(): Set<string> {
  const routes = new Set<string>(['/']);
  const walk = (dir: string, prefix: string) => {
    for (const name of readdirSync(dir, { withFileTypes: true })) {
      const full = `${dir}/${name.name}`;
      if (name.isDirectory()) walk(full, `${prefix}/${name.name}`);
      else if (name.name === '+page.svelte' || name.name === '+error.svelte') {
        const base = prefix.replaceAll(/\[([^\]]+)\]/g, '{$1}') || '/';
        routes.add(base === '' ? '/' : base);
      }
    }
  };
  walk('src/routes', '');
  return routes;
}

/** 原型 router.js 中的全部路由 pattern（仅测试引用，作为设计回归基线）。 */
const PROTOTYPE_ROUTES = [
  '/', '/articles', '/boards', '/boards/{slug}', '/tags', '/tags/{name}',
  '/topics/{id}', '/users/{name}', '/publish', '/notifications', '/favorites',
  '/shop', '/activity', '/me/closet', '/search', '/settings', '/login',
  '/register', '/forgot-password', '/403', '/404', '/429', '/admin',
  '/admin/users', '/admin/roles', '/admin/content', '/admin/posts',
  '/admin/boards', '/admin/tags', '/admin/attachments', '/admin/download-billing',
  '/admin/ai', '/admin/video', '/admin/storage', '/admin/notifications',
  '/admin/audit', '/admin/reports', '/admin/reports/{id}', '/admin/points',
  '/admin/levels', '/admin/themes', '/admin/plugins', '/admin/oauth',
  '/admin/marketplace', '/admin/shop', '/admin/activity', '/admin/settings'
];

function listSourceFiles(dir: string, acc: string[] = []): string[] {
  for (const name of readdirSync(dir, { withFileTypes: true })) {
    const full = `${dir}/${name.name}`;
    if (name.isDirectory()) listSourceFiles(full, acc);
    else if (/\.(svelte|ts|js)$/.test(name.name) && !name.name.endsWith('.test.ts')) acc.push(full);
  }
  return acc;
}

describe('M03-UI-09 原型 → 生产路由矩阵', () => {
  it('矩阵覆盖原型 router.js 全部路由（无遗漏，设计回归基线）', () => {
    const matrixPrototypes = PROTOTYPE_ROUTE_MATRIX.map((e) => e.prototype);
    for (const route of PROTOTYPE_ROUTES) {
      expect(matrixPrototypes).toContain(route);
    }
    // 矩阵无多余行、无遗漏行（与原型一一对应；排序比较避免分组顺序耦合）。
    expect([...matrixPrototypes].sort()).toEqual([...PROTOTYPE_ROUTES].sort());
  });

  it('shipped 路由的 +page.svelte / +error.svelte 存在于生产 routes', () => {
    const routes = productionRoutes();
    for (const entry of PROTOTYPE_ROUTE_MATRIX.filter((e) => e.status === 'shipped')) {
      if (entry.production.startsWith('+error.svelte')) {
        expect(existsSync('src/routes/+error.svelte'), '+error.svelte 必须存在').toBe(true);
        continue;
      }
      if (entry.production.includes('?')) continue; // 查询参数形态（如 /search?tag=）
      const production = entry.production.replaceAll(/\[([^\]]+)\]/g, '{$1}');
      expect(routes.has(production), `${entry.production} 应存在 +page.svelte（shipped）`).toBe(true);
    }
  });

  it('生产源码不导入原型（prototype/）的 mock/store/脚本数据源', () => {
    const offenders: string[] = [];
    for (const file of listSourceFiles('src')) {
      const content = readFileSync(file, 'utf8');
      if (/(from\s+['"]\.\.?\/.*prototype|import\s+.*prototype|require\(.*prototype)/.test(content)) {
        offenders.push(file);
      }
    }
    expect(offenders, '生产源码不得引用原型 mock/store').toEqual([]);
    // 显式防御：原型数据文件路径不得以引号包裹的引用形态出现（import/动态
    // import/require），描述性注释提及原型出处不在此列。
    for (const file of listSourceFiles('src')) {
      const content = readFileSync(file, 'utf8');
      expect(content).not.toMatch(/['"][^\n'"]*prototype\/js\/(mock|store|pages\d*)[^\n'"]*['"]/);
    }
  });

  it('矩阵中 planned 路由都标注了交付里程碑', () => {
    for (const entry of PROTOTYPE_ROUTE_MATRIX.filter((e) => e.status === 'planned')) {
      expect(entry.milestone).toMatch(/^M\d+$/);
    }
  });
});
