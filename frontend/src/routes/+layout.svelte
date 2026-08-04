<script lang="ts">
  let { children } = $props();
</script>

<svelte:head>
  <meta name="theme-color" content="#111827" />
</svelte:head>

<div class="shell">
  <header class="site-header">
    <a class="brand" href="/">BBLBB</a>
    <span class="environment">SvelteKit SSR</span>
  </header>

  <main>
    {@render children()}
  </main>
</div>

<style>
  :global(*) { box-sizing: border-box; }
  :global(body) { margin: 0; background: #f3f4f6; color: #111827; font-family: system-ui, sans-serif; }
  .shell { min-height: 100vh; }
  .site-header { align-items: center; background: #111827; color: #fff; display: flex; justify-content: space-between; padding: 1rem max(1.25rem, calc((100vw - 56rem) / 2)); }
  .brand { color: inherit; font-weight: 700; letter-spacing: 0.08em; text-decoration: none; }
  .environment { color: #cbd5e1; font-size: 0.875rem; }
  main { margin: 0 auto; max-width: 56rem; padding: 4rem 1.25rem; }
</style>
