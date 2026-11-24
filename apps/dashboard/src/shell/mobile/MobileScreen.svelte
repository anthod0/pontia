<script lang="ts">
  import type { Snippet } from 'svelte'
  import { edgeSwipeBack } from '$lib/mobile/gestures'
  import MobileHeader from './MobileHeader.svelte'
  let { title, subtitle, back, modal = false, children }: { title: string; subtitle?: string; modal?: boolean; back: () => void; children: Snippet } = $props()
</script>

<section class="mobile-screen absolute inset-0 z-20 flex flex-col bg-surface" data-modal={modal} use:edgeSwipeBack={modal ? undefined : back}>
  <MobileHeader {title} {subtitle} {back} {modal} />
  <div class="min-h-0 flex-1 overflow-y-auto px-4 pb-4 [touch-action:pan-y_pinch-zoom]">{@render children()}</div>
</section>
