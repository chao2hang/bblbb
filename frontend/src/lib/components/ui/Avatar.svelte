<script lang="ts">
  const PALETTE: [string, string][] = [
    ['#0969DA', '#54AEFF'],
    ['#8250DF', '#B083F0'],
    ['#1A7F37', '#4AC26B'],
    ['#BF8700', '#E3B341'],
    ['#CF222E', '#FF8182'],
    ['#0E8A16', '#57AB5A'],
    ['#DA3633', '#F778BA'],
    ['#8250DF', '#8C959F']
  ];
  const SIZES: Record<string, number> = { xs: 20, sm: 24, md: 32, lg: 40, xl: 64, '2xl': 96 };

  let { name = '?', size = 'md', title }: { name?: string; size?: string; title?: string } = $props();

  const initial = $derived((name && name.charAt(0)) || '?');
  const idx = $derived(
    Math.abs(String(name || '?').split('').reduce((a, c) => a + c.charCodeAt(0), 0)) % PALETTE.length
  );
  const c1 = $derived(PALETTE[idx][0]);
  const c2 = $derived(PALETTE[idx][1]);
  const px = $derived(SIZES[size] || 32);
</script>

<span
  class="avatar avatar-{size}"
  title={title ?? name}
  role="img"
  aria-label={name}
  style="width:{px}px;height:{px}px;font-size:{Math.round(px * 0.42)}px;background:linear-gradient(135deg,{c1},{c2});color:#fff;"
  >{initial}</span
>
