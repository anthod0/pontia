<script lang="ts">
  import type { Snippet } from 'svelte'
  import { t } from '$lib/i18n'
  import * as Dialog from '$lib/components/ui/dialog/index.js'
  import { Lightbox } from 'picola/svelte'
  import 'picola/picola.css'
  import XIcon from 'phosphor-svelte/lib/XIcon'

  export type LightboxImage = { name: string; src: string | null }

  let { open = $bindable(false), index = $bindable(0), images, label, children }: {
    open?: boolean
    index?: number
    images: LightboxImage[]
    label?: string
    /** Shown over the active image while its `src` is null, such as a loading or error state. */
    children?: Snippet
  } = $props()

  const active = $derived(images[index])
  const slides = $derived(images.map(image => ({ src: image.src ?? '', alt: image.name })))
</script>

<Dialog.Root bind:open>
  <Dialog.Content class="inset-0 block max-w-none translate-none rounded-none bg-black/90 p-0 text-white ring-0 sm:max-w-none" showCloseButton={false}>
    <Dialog.Title class="sr-only">{active.name}</Dialog.Title>
    <Lightbox
      inline
      bind:index
      {slides}
      zoom={{ max: 8 }}
      labels={{ dialog: label ?? active.name, zoom: $t('files.toggleZoom'), prev: $t('files.previousImage'), next: $t('files.nextImage') }}
    >
      {#snippet toolbar()}
        <Dialog.Close class="pcl__button" aria-label={$t('common.close')}><XIcon class="size-5" /></Dialog.Close>
      {/snippet}
    </Lightbox>
    {#if !active.src}
      <div class="pointer-events-none absolute inset-0 flex items-center justify-center p-8 text-center">
        <div class="pointer-events-auto flex flex-col items-center gap-4">{@render children?.()}</div>
      </div>
    {/if}
  </Dialog.Content>
</Dialog.Root>
