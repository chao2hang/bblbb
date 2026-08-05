<script lang="ts">
  // M02-UX-02：冷却倒计时按钮。
  // - `cooldown > 0` 时禁用按钮并显示剩余秒数（JS 每秒递减）；
  // - `attempt` 变化强制重启计时（处理“同秒数重复重启”，如连续两次 60s）；
  // - 无 JS 时 SSR 渲染初始禁用态；冷却是客户端渐进增强，真正的限流裁决
  //   始终在后端（429 + Retry-After）。
  let {
    cooldown = 0,
    attempt = 0,
    text = '重新发送',
    disabled = false,
    class: klass = '',
    ...rest
  }: {
    cooldown?: number;
    attempt?: number;
    text?: string;
    disabled?: boolean;
    class?: string;
  } & Record<string, unknown> = $props();

  let remaining = $state(0);

  $effect(() => {
    void attempt; // 依赖 attempt：变化即重启（同秒数重复计时）
    if (cooldown <= 0) {
      remaining = 0;
      return;
    }
    remaining = cooldown;
    const timer = setInterval(() => {
      remaining = Math.max(0, remaining - 1);
    }, 1000);
    return () => clearInterval(timer);
  });

  const label = $derived(remaining > 0 ? `${text}（${remaining} 秒）` : text);
</script>

<button class={klass} {...rest} type="submit" disabled={disabled || remaining > 0}>
  {label}
</button>
