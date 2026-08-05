<script lang="ts">
  // M00-FRONTEND-07 测试用夹具：把 Field 与真实输入控件按表单错误关联接线。
  import Field from '$lib/components/ui/Field.svelte';

  let { label = '', error = '', hint = '' }: { label?: string; error?: string; hint?: string } = $props();
</script>

<Field {label} {error} {hint} id="f-email">
  <input
    id="f-email"
    type="email"
    aria-describedby={error ? 'f-email-error' : hint ? 'f-email-hint' : undefined}
    aria-invalid={error ? 'true' : undefined}
  />
</Field>