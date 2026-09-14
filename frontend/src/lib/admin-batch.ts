// M18-ADMIN-BATCH-03：批量操作服务端辅助。
//
// 约定：批量 action 通过隐藏表单字段接收选中行：
// - `ids`：逗号分隔或重复字段均可（`parseBatchIds` 同时兼容两种）；
// - `versions`：可选，与 ids 顺序一一对应的乐观锁版本（逗号分隔），
//   存在时逐条以 If-Match 提交；某条缺版本则该条不带 If-Match。
//
// 批量语义 = 循环调用该页面单条 action 已在用的同一个后端端点（不新增后端
// 路由），逐条 try/catch 汇总成败，最终以「成功 N 项 / 失败 M 项（附首个失败
// 原因）」返回，部分失败走 fail(500) 让 Toast 以错误色呈现。

export interface BatchEntry {
  id: string;
  /** 乐观锁版本；null = 该行未提供版本（不加 If-Match）。 */
  version: string | null;
}

export interface BatchFailure {
  id: string;
  message: string;
}

export interface BatchOutcome {
  okCount: number;
  failures: BatchFailure[];
}

/** 解析批量 id 列表：兼容重复字段（getAll）与逗号分隔两种提交方式，去重保序。 */
export function parseBatchIds(form: FormData, key = 'ids'): string[] {
  const out: string[] = [];
  for (const raw of form.getAll(key)) {
    for (const part of String(raw).split(',')) {
      const id = part.trim();
      if (id && !out.includes(id)) out.push(id);
    }
  }
  return out;
}

/** 解析 id + 可选 version 配对（versions 逗号分隔，与 ids 顺序对齐）。 */
export function parseBatchEntries(form: FormData, idKey = 'ids', versionKey = 'versions'): BatchEntry[] {
  const ids = parseBatchIds(form, idKey);
  const versionsRaw = String(form.get(versionKey) ?? '').trim();
  const versions = versionsRaw ? versionsRaw.split(',') : [];
  return ids.map((id, i) => {
    const v = (versions[i] ?? '').trim();
    return { id, version: v || null };
  });
}

/** 空选择守卫：批量表单未携带任何 id 时返回可读错误。 */
export function emptyBatchSelection(): BatchOutcome {
  return { okCount: 0, failures: [{ id: '-', message: '未选择任何条目' }] };
}

/** 汇总批量结果文案；`label` 为操作名（如「批量启用」）。 */
export function batchResult(
  outcome: BatchOutcome,
  label: string
): { ok: boolean; status: number; message: string } {
  const { okCount, failures } = outcome;
  if (failures.length === 0 && okCount > 0) {
    return { ok: true, status: 200, message: `${label}完成：成功 ${okCount} 项` };
  }
  if (okCount === 0 && failures.length > 0) {
    const first = failures[0];
    return {
      ok: false,
      status: 500,
      message: `${label}失败：${failures.length} 项均未成功（首个失败 ${first.id}：${first.message}）`
    };
  }
  const first = failures[0];
  return {
    ok: false,
    status: 500,
    message: `${label}部分成功：成功 ${okCount} 项，失败 ${failures.length} 项（首个失败 ${first.id}：${first.message}）`
  };
}
