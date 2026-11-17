<script lang="ts">
  import type { Snippet } from 'svelte'
  import { t } from '$lib/i18n'
  import * as Dialog from '$lib/components/ui/dialog/index.js'
  import { Lightbox } from 'picola/svelte'
  import 'picola/picola.css'
  import XIcon from 'phosphor-svelte/lib/XIcon'

  let { open = $bindable(false), src, name, label = name, children }: {
    open?: boolean
    src: string | null
    name: string
    label?: string
    /** Shown instead of the image while `src` is null, such as a loading or error state. */
    children?: Snippet
  } = $props()
</script>

<Dialog.Root bind:open>
  <Dialog.Content class="inset-0 block max-w-none translate-none rounded-none bg-black/90 p-0 text-white ring-0 sm:max-w-none" showCloseButton={false}>
    <Dialog.Title class="sr-only">{name}</Dialog.Title>
    {#if src}
      <Lightbox
        inline
        slides={[{ src, alt: name }]}
        zoom={{ max: 8 }}
        counter={false}
        labels={{ dialog: label, zoom: $t('files.toggleZoom') }}
      >
        {#snippet toolbar()}
          <Dialog.Close class="pcl__button" aria-label={$t('common.close')}><XIcon class="size-5" /></Dialog.Close>
        {/snippet}
      </Lightbox>
    {:else}
      <div class="flex size-full flex-col items-center justify-center gap-4 p-8 text-center">
        <Dialog.Close class="absolute top-3 right-3 rounded-md p-2 hover:bg-white/10" aria-label={$t('common.close')}><XIcon class="size-5" /></Dialog.Close>
        {@render children?.()}
      </div>
    {/if}
  </Dialog.Content>
</Dialog.Root>
