<script lang="ts">
  // GAP-FIX（社交域·API Key）：/apikeys——密钥管理 SSR。
  //
  // - 表格：名称/prefix/scopes/创建时间/最近使用/状态（+ 撤销）；
  // - 创建表单：name + scope 复选（白名单）→ create 成功后一次性明文
  //   密钥展示框（仅此一次显示，请立即保存）+ 复制按钮；
  // - 撤销：form action + use:enhance（提交前 confirm 确认）；
  // - 无 JS 基线：原生 form POST 整页刷新，form.message 状态行可见。
  import { enhance } from '$app/forms';
  import { invalidateAll } from '$app/navigation';
  import Button from '$lib/components/ui/Button.svelte';
  import EmptyState from '$lib/components/ui/EmptyState.svelte';
  import ProblemState from '$lib/components/ProblemState.svelte';
  import Badge from '$lib/components/ui/Badge.svelte';
  import { show } from '$lib/ui/toast';
  import { formatTime } from '$lib/utils';
  import type { ApiKeysActionData, ApiKeysPageData } from './+page.server';

  let { data, form }: { data: ApiKeysPageData; form?: ApiKeysActionData | null } = $props();

  /** 一次性密钥框是否已关闭（新密钥创建后重置）。 */
  let keyDismissed = $state(false);
  let creating = $state(false);
  let revokingId = $state<string | null>(null);
  let copied = $state(false);

  const actionMessage = $derived(form?.message ?? null);
  const createdKey = $derived(form?.created ?? null);

  /** 后端时间戳为毫秒（M01-DB-08），formatTime 口径为秒。 */
  function toSeconds(ts: number | null | undefined): number | null {
    if (typeof ts !== 'number' || !Number.isFinite(ts) || ts <= 0) return null;
    return ts > 1e11 ? Math.floor(ts / 1000) : ts;
  }

  /** 复制一次性密钥（剪贴板 API 失败时提示手动复制）。 */
  async function copyKey(): Promise<void> {
    if (!createdKey) return;
    try {
      await navigator.clipboard.writeText(createdKey.key);
      copied = true;
      show('已复制到剪贴板', 'success');
    } catch {
      show('复制失败，请手动选中密钥复制', 'warning');
    }
  }
</script>

<svelte:head>
  <title>API 密钥 — BBLBB</title>
</svelte:head>

<div class="container page-content">
  <!-- 原型对齐（prototype/pages/apikeys.html）：app-route-head，无面包屑。 -->
  <div class="app-route-head">
    <div class="app-route-head__copy">
      <span class="app-kicker">ACCOUNT / DEVELOPER</span>
      <h1 tabindex="-1">API 密钥</h1>
      <p>按最小 scope 管理个人 API 访问凭证</p>
    </div>
  </div>

  {#if data.problem}
    <ProblemState problem={data.problem} />
  {:else}
    <!-- 创建表单 -->
    <div class="card">
      <div class="card-header"><span class="card-title">创建密钥</span></div>
      <div class="card-body" style="display:flex;flex-direction:column;gap:var(--space-4);">
        <!-- M18-MISC-04：信息横幅（对齐原型同款提示） -->
        <div class="app-notice" role="note" style="padding:10px 14px;border-radius:var(--radius-md);background:var(--color-bg-subtle, rgba(0,0,0,0.04));border-left:3px solid var(--color-brand);">
          <p style="margin:0;font-size:var(--text-sm);color:var(--color-text-secondary);">
            密钥只在创建时显示一次；撤销会立即使旧密钥失效。
          </p>
        </div>
        {#if actionMessage && !createdKey}
          <p class="input-hint" role="status" style="margin:0;">{actionMessage}</p>
        {/if}
        <form
          class="apikey-form"
          method="POST"
          action="?/create"
          use:enhance={() => {
            creating = true;
            return async ({ result, update }) => {
              creating = false;
              if (result.type === 'success') {
                keyDismissed = false;
                copied = false;
                show('密钥已创建', 'success');
                await update();
                // 刷新表格（新密钥出现在列表中；key 只在本页展示这一次）。
                await invalidateAll();
              } else {
                if (result.type === 'failure') {
                  show(
                    String((result.data as ApiKeysActionData | undefined)?.message ?? '创建失败'),
                    'danger'
                  );
                }
                await update();
              }
            };
          }}
        >
          <input type="hidden" name="client_request_id" value={data.clientRequestId} />
          <div class="input-wrapper">
            <label class="input-label" for="apikey-name">密钥名称</label>
            <input
              id="apikey-name"
              class="input-field"
              type="text"
              name="name"
              maxlength="64"
              required
              placeholder="例如：我的脚本"
            />
            <p class="input-hint">1-64 字符，用于识别密钥用途。</p>
          </div>
          <fieldset style="border:none;margin:0;padding:0;">
            <legend class="input-label" style="margin-bottom:var(--space-2);">权限范围（scopes）</legend>
            <div class="apikey-scopes">
              {#each ['posts:read', 'drafts:write', 'notifications:read', 'me:read'] as scope (scope)}
                <label class="apikey-scope">
                  <input type="checkbox" name="scopes" value={scope} />
                  <span>{scope}</span>
                </label>
              {/each}
            </div>
            <p class="input-hint">只勾选需要的权限；密钥创建后权限不可修改，只能重建。</p>
          </fieldset>
          <div>
            <Button type="submit" text={creating ? '创建中…' : '创建密钥'} variant="primary" size="sm" disabled={creating} />
          </div>
        </form>

        <!-- 一次性明文密钥展示框（仅此一次） -->
        {#if createdKey && !keyDismissed}
          <div class="apikey-once" role="alert">
            <div class="apikey-once-title">
              密钥「{createdKey.name}」已创建 —— 仅此一次显示，请立即保存
            </div>
            <code class="apikey-once-key">{createdKey.key}</code>
            <p class="apikey-once-hint">
              后端只保存密钥的 SHA-256 哈希（hash），明文离开本页后将无法再次找回。
              {#if createdKey.scopes.length > 0}权限：{createdKey.scopes.join('、')}{/if}
            </p>
            <div style="display:flex;gap:var(--space-2);flex-wrap:wrap;">
              <Button text={copied ? '已复制' : '复制密钥'} variant="primary" size="sm" onclick={() => void copyKey()} />
              <Button text="我已保存，关闭" variant="ghost" size="sm" onclick={() => (keyDismissed = true)} />
            </div>
          </div>
        {/if}
      </div>
    </div>

    <!-- 密钥列表 -->
    <div class="card" style="margin-top:var(--space-5);">
      <div class="card-header"><span class="card-title">我的密钥（{data.keys.length}）</span></div>
      <div class="card-body" style="padding:0;">
        {#if data.keys.length === 0}
          <div style="padding:var(--space-6);">
            <EmptyState
              icon="key"
              title="还没有 API 密钥"
              desc="在上方创建一个密钥，用于以自己的身份调用本站 API"
            />
          </div>
        {:else}
          <div style="overflow-x:auto;">
            <table class="table" aria-label="API 密钥列表">
              <thead>
                <tr>
                  <th>名称</th>
                  <th>前缀</th>
                  <th>权限</th>
                  <th>创建时间</th>
                  <th>最近使用</th>
                  <th>状态</th>
                  <th>操作</th>
                </tr>
              </thead>
              <tbody>
                {#each data.keys as key (key.id)}
                  {@const revoked = typeof key.revoked_at === 'number' && key.revoked_at > 0}
                  <tr>
                    <td>{key.name}</td>
                    <td><code style="font-size:var(--text-sm);">{key.prefix}…</code></td>
                    <td>
                      <span style="font-size:var(--text-sm);">{key.scopes.length > 0 ? key.scopes.join('、') : '—'}</span>
                    </td>
                    <td><span class="text-secondary" style="font-size:var(--text-sm);white-space:nowrap;">{formatTime(toSeconds(key.created_at))}</span></td>
                    <td><span class="text-secondary" style="font-size:var(--text-sm);white-space:nowrap;">{key.last_used_at ? formatTime(toSeconds(key.last_used_at)) : '从未使用'}</span></td>
                    <td>
                      {#if revoked}
                        <Badge text="已撤销" type="danger" />
                      {:else}
                        <Badge text="有效" type="success" />
                      {/if}
                    </td>
                    <td>
                      {#if revoked}
                        <span class="text-secondary" style="font-size:var(--text-sm);">—</span>
                      {:else}
                        <form
                          method="POST"
                          action="?/revoke"
                          use:enhance={({ cancel }) => {
                            if (!window.confirm(`确定撤销密钥「${key.name}」吗？撤销后立即失效，不可恢复。`)) {
                              cancel();
                              return;
                            }
                            revokingId = key.id;
                            return async ({ result, update }) => {
                              revokingId = null;
                              if (result.type === 'success') {
                                show('密钥已撤销，立即失效', 'success');
                                await update();
                                await invalidateAll();
                              } else {
                                if (result.type === 'failure') {
                                  show(
                                    String(
                                      (result.data as ApiKeysActionData | undefined)?.message ??
                                        '撤销失败'
                                    ),
                                    'danger'
                                  );
                                }
                                await update();
                              }
                            };
                          }}
                        >
                          <input type="hidden" name="id" value={key.id} />
                          <Button
                            type="submit"
                            text={revokingId === key.id ? '撤销中…' : '撤销'}
                            variant="ghost"
                            size="sm"
                            disabled={revokingId === key.id}
                          />
                        </form>
                      {/if}
                    </td>
                  </tr>
                {/each}
              </tbody>
            </table>
          </div>
        {/if}
      </div>
    </div>
  {/if}
</div>

<style>
  .apikey-form {
    display: flex;
    flex-direction: column;
    gap: var(--space-4);
  }

  .apikey-scopes {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-3);
  }

  .apikey-scope {
    display: inline-flex;
    align-items: center;
    gap: var(--space-2);
    padding: var(--space-2) var(--space-3);
    border: var(--border-default);
    border-radius: var(--radius-md);
    font-size: var(--text-sm);
    cursor: pointer;
  }

  .apikey-once {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
    padding: var(--space-4);
    border: 1px solid var(--color-warning);
    border-radius: var(--radius-md);
    background: var(--color-warning-soft);
  }

  .apikey-once-title {
    font-weight: var(--weight-medium);
  }

  .apikey-once-key {
    display: block;
    padding: var(--space-3);
    border-radius: var(--radius-md);
    background: var(--color-bg-inset);
    font-size: var(--text-sm);
    word-break: break-all;
    user-select: all;
  }

  .apikey-once-hint {
    margin: 0;
    font-size: var(--text-sm);
    color: var(--color-text-secondary);
  }
</style>
