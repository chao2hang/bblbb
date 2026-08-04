<script lang="ts">
  let { label, value, change = null }: { label: string; value: string; change?: string | null } = $props();
</script>

<div class="stat-card">
  <div class="stat-card-value">{value}</div>
  <div class="stat-card-label">{label}</div>
  {#if change}<div class="stat-card-change">{change}</div>{/if}
</div>
