<script lang="ts">
  let { text, type = 'neutral' }: { text: string; type?: string } = $props();
</script>

<span class="badge badge-{type}">{text}</span>
