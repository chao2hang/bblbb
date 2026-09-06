<script lang="ts">
  // GAP-FIX（M17-GAPFIX-07·组件封装）：页面标题（原型 sr-only h1 同构）。
  // 所有管理页统一入口，保证无 JS/读屏器下每页有标题。
  let { title }: { title: string } = $props();
</script>

<h1 class="sr-only">{title}</h1>
