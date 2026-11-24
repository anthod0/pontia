<script lang="ts">
  import type { Snippet } from 'svelte'
  import ArrowSquareOutIcon from 'phosphor-svelte/lib/ArrowSquareOutIcon'
  import type { LocalhostPreviewTarget } from '$lib/localhostPreview'

  let { target, open, title, children }: {
    target: LocalhostPreviewTarget
    open: (target: LocalhostPreviewTarget) => void
    title?: string | null
    children?: Snippet
  } = $props()
</script>

<a href={target.rawHref} {title} target="_blank" rel="noopener noreferrer" onclick={(event) => {
  event.preventDefault()
  open(target)
}}>
  {@render children?.()}
  <ArrowSquareOutIcon aria-hidden="true" class="ml-1 inline-block size-3.5 align-[-0.125em]" />
</a>
