<script lang="ts">
  import { type Snippet } from 'svelte'
  import FileTextIcon from 'phosphor-svelte/lib/FileTextIcon'
  import { Button } from '$lib/components/ui/button/index.js'
  import { t } from '$lib/i18n'
  import { useDeviceRuntime } from '../../../../device/context'
  import { useFilePreview } from '../../../../components/chat/filePreviewContext'

  let { sessionId, path, title, children }: {
    sessionId: string
    path: string
    title?: string | null
    children?: Snippet
  } = $props()

  const device = useDeviceRuntime()
  const preview = useFilePreview()
  const name = $derived(path.slice(path.lastIndexOf('/') + 1))
</script>

<Button
  variant="link"
  class="inline h-auto rounded-sm p-0 align-baseline font-normal whitespace-normal underline"
  title={title || path}
  aria-label={$t('files.previewNamed', { name })}
  onclick={() => preview.set({ device, sessionId, path })}
>
  {@render children?.()}
  <FileTextIcon class="ml-1 inline-block size-3.5 align-[-0.125em]" aria-hidden="true" />
</Button>
