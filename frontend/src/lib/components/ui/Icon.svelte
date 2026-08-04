<script lang="ts">
  import { icons } from './icons';

  let {
    name,
    size = 16,
    class: klass = ''
  }: { name: string; size?: number; class?: string } = $props();
</script>

<svg
  class="icon icon-{name} {klass}"
  width={size}
  height={size}
  viewBox="0 0 24 24"
  fill="none"
  stroke="currentColor"
  stroke-width="2"
  stroke-linecap="round"
  stroke-linejoin="round"
  aria-hidden="true"
  focusable="false"
>
  {@html icons[name] || ''}
</svg>
