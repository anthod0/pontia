<script lang="ts">
  import { t } from '$lib/i18n'
  import * as Tabs from '$lib/components/ui/tabs/index.js'
  import UserFilePanel from './memory/UserFilePanel.svelte'
  import MemoryBrowser from './memory/MemoryBrowser.svelte'

  let tab = $state('memories')
</script>

<section class="space-y-4">
  <p class="text-sm text-muted-foreground">{$t('memory.description')}</p>
  <Tabs.Root bind:value={tab}>
    <Tabs.List variant="line" aria-label={$t('memory.views')}>
      <Tabs.Trigger value="memories">{$t('memory.entries')}</Tabs.Trigger>
      <Tabs.Trigger value="user">{$t('memory.userTab')}</Tabs.Trigger>
    </Tabs.List>
    <Tabs.Content value="memories" class="pt-3"><MemoryBrowser /></Tabs.Content>
    <Tabs.Content value="user" class="pt-3"><UserFilePanel /></Tabs.Content>
  </Tabs.Root>
</section>
