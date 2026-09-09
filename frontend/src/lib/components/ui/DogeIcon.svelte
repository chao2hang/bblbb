<!-- BBLBB 经典全网正版「狗头」/「狗头保命」高清表情 -->
<script lang="ts">
  import { DOGE_DATA_URI } from './emojis-data';

  let {
    size = 18,
    class: klass = ''
  }: { size?: number; class?: string } = $props();
</script>

<img
  src={DOGE_DATA_URI}
  alt="狗头"
  width={size}
  height={size}
  class="doge-icon {klass}"
  draggable="false"
/>

<style>
  .doge-icon {
    display: inline-block;
    vertical-align: -0.15em;
    object-fit: contain;
    flex-shrink: 0;
    user-select: none;
  }
</style>
