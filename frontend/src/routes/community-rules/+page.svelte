<script lang="ts">
  import PageTitle from '$lib/components/PageTitle.svelte';
  import { COMMUNITY_RULES_VERSION, rules } from '$lib/community-rules';
</script>

<svelte:head>
  <meta name="description" content="BBLBB 社区参与与内容发布规则。" />
</svelte:head>

<PageTitle title="社区规则" />

<main class="rules-page">
  <header class="rules-heading">
    <p class="eyebrow">BBLBB COMMUNITY</p>
    <h1>社区规则</h1>
    <p class="lead">让讨论有分歧但不失尊重，让内容有价值也有边界。</p>
    <p class="version">适用于注册与社区参与 · 版本 {COMMUNITY_RULES_VERSION}</p>
  </header>

  {#each rules as section (section.title)}
    <section aria-labelledby={`rules-${section.title}`}>
      <h2 id={`rules-${section.title}`}>{section.title}</h2>
      <ul>
        {#each section.items as item (item)}
          <li>{item}</li>
        {/each}
      </ul>
    </section>
  {/each}

  <p class="closing">注册即表示你确认已阅读并同意本版本社区规则。规则版本或内容有重要变化时，平台应在产品内告知用户。</p>
</main>

<style>
  .rules-page {
    max-width: 760px;
    margin: 0 auto;
    padding: var(--space-8) var(--space-4) var(--space-12);
    color: var(--color-text-primary);
  }

  .rules-heading {
    padding: var(--space-8) 0 var(--space-6);
    border-bottom: var(--border-default);
    margin-bottom: var(--space-6);
  }

  .eyebrow {
    margin: 0 0 var(--space-2);
    color: var(--color-text-secondary);
    font-size: var(--text-xs);
    font-weight: 700;
    letter-spacing: 0.12em;
  }

  h1 {
    margin: 0;
    font-size: clamp(2rem, 6vw, 3rem);
    line-height: 1.12;
  }

  .lead {
    max-width: 36rem;
    margin: var(--space-4) 0 0;
    color: var(--color-text-secondary);
    font-size: var(--text-lg);
  }

  .version {
    margin: var(--space-3) 0 0;
    color: var(--color-text-tertiary);
    font-size: var(--text-sm);
  }

  section {
    padding: var(--space-5) 0;
    border-bottom: var(--border-default);
  }

  h2 {
    margin: 0 0 var(--space-3);
    font-size: var(--text-xl);
  }

  ul {
    display: grid;
    gap: var(--space-3);
    margin: 0;
    padding-left: 1.25rem;
    color: var(--color-text-secondary);
    line-height: 1.8;
  }

  li::marker {
    color: var(--color-brand);
  }

  .closing {
    margin: var(--space-6) 0 0;
    padding: var(--space-4);
    border-left: 3px solid var(--color-brand);
    background: var(--color-surface-muted);
    line-height: 1.75;
  }
</style>
