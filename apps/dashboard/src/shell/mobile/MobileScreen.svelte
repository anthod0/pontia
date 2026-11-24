<script lang="ts">
  import type { Snippet } from 'svelte'
  import { edgeSwipeBack } from '$lib/mobile/gestures'
  import MobileHeader from './MobileHeader.svelte'
  let { title, back, modal = false, children }: { title: string; modal?: boolean; back: () => void; children: Snippet } = $props()
</script>

<section class="mobile-screen absolute inset-0 z-20 flex flex-col bg-surface" data-modal={modal} use:edgeSwipeBack={modal ? undefined : back}>
  <MobileHeader {title} {back} {modal} />
  <div class="min-h-0 flex-1 overflow-y-auto px-4 pb-4">{@render children()}</div>
</section>
