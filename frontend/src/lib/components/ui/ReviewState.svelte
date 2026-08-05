<script lang="ts">
  // 审核中状态组件（M00-FRONTEND-05）：内容/用户处于审核流程时展示。
  import Icon from './Icon.svelte';

  let {
    title = '内容审核中',
    desc = '该内容正在接受社区审核，通过后将自动对符合条件的人可见。'
  }: { title?: string; desc?: string } = $props();
</script>

<div class="empty-state">
  <div class="empty-state-icon"><Icon name="shield" size={40} /></div>
  <div class="empty-state-title">{title}</div>
  {#if desc}<div class="empty-state-desc">{desc}</div>{/if}
</div>