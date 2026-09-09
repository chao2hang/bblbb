<!-- BBLBB 经典百度贴吧原版「滑稽」高清表情 -->
<script lang="ts">
  import { HUAJI_DATA_URI } from './emojis-data';

  let {
    size = 18,
    class: klass = ''
  }: { size?: number; class?: string } = $props();
</script>

<img
  src={HUAJI_DATA_URI}
  alt="滑稽"
  width={size}
  height={size}
  class="huaji-icon {klass}"
  draggable="false"
/>

<style>
  .huaji-icon {
    display: inline-block;
    vertical-align: -0.15em;
    object-fit: contain;
    flex-shrink: 0;
    user-select: none;
  }
</style>
