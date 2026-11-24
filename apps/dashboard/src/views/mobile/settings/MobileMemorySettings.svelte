<script lang="ts">
  import { t } from '$lib/i18n'
  import * as Tabs from '$lib/components/ui/tabs/index.js'
  import UserFilePanel from '../../../pages/memory/UserFilePanel.svelte'
  import MemoryBrowser from '../../../pages/memory/MemoryBrowser.svelte'

  import { useMobileStack } from '$lib/mobile/overlayStack.svelte'
  const stack = useMobileStack()!
  let editorId: string
  let tab = $state('memories')
</script>

<section class="space-y-4">
  <Tabs.Root bind:value={tab}>
    <Tabs.List class="w-full" aria-label={$t('memory.views')}>
      <Tabs.Trigger value="memories">{$t('memory.entries')}</Tabs.Trigger>
      <Tabs.Trigger value="user">{$t('memory.userTab')}</Tabs.Trigger>
    </Tabs.List>
    <Tabs.Content value="memories" class="pt-3"><MemoryBrowser /></Tabs.Content>
    <Tabs.Content value="user" class="pt-3"
      ><UserFilePanel
        openEditor={(content) => { editorId = stack.open({ title: 'USER.md', content }) }}
        closeEditor={() => stack.release(editorId)}
      /></Tabs.Content
    >
  </Tabs.Root>
</section>
