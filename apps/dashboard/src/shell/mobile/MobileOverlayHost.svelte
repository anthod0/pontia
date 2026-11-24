<script lang="ts">
  import type { MobileOverlayStack } from '$lib/mobile/overlayStack.svelte'
  import MobileScreen from './MobileScreen.svelte'
  let { stack }: { stack: MobileOverlayStack } = $props()
</script>

{#each stack.layers as layer, i (layer.id)}
  {#if layer.content}
    <div inert={i < stack.layers.length - 1}>
      <MobileScreen subtitle={layer.subtitle} modal={layer.modal} title={layer.title ?? ''} back={() => stack.back()}>{@render layer.content()}</MobileScreen>
    </div>
  {/if}
{/each}
