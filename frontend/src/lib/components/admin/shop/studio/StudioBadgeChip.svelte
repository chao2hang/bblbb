<!-- M07-SHOP-STUDIO：徽章/称号/反应包的章面 chip 预览。
  图标来自固定白名单（Icon.svelte lucide 名），颜色为 #rrggbb（style-draft
  预检 + 服务端 schema 双重校验），不接受任意 CSS。 -->
<script lang="ts">
  import Icon from '$lib/components/ui/Icon.svelte';
  import { ICON_OPTIONS } from './style-draft';

  let {
    icon = 'sparkles',
    color = '#f472b6',
    name = '预览'
  }: {
    icon?: string;
    color?: string;
    name?: string;
  } = $props();

  const SAFE_ICONS = new Set(ICON_OPTIONS.map((o) => o.value));
  const iconName = $derived(SAFE_ICONS.has(icon) ? icon : 'sparkles');
  const safeColor = $derived(/^#[0-9a-fA-F]{6}$/.test(color) ? color : '#f472b6');
</script>

<span class="studio-badge-chip" style="color:{safeColor};border-color:{safeColor};" title={name}>
  <Icon name={iconName} size={13} />
  <span class="studio-badge-chip__label">{name}</span>
</span>

<style>
  .studio-badge-chip {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    padding: 4px 10px;
    border: 1px solid;
    border-radius: var(--radius-sm, 6px);
    font-size: var(--text-xs, 12px);
    font-weight: 600;
    line-height: 1.4;
    background: var(--color-bg-card);
    white-space: nowrap;
  }
</style>
