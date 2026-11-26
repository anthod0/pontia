<script lang="ts">
  import { t } from '$lib/i18n'
  import BotGitTab from '../../../components/bots/BotGitTab.svelte'
  import RecentFileList from '../../../components/chat/files/RecentFileList.svelte'
  import type { RecentFile, createRecentFiles } from '$lib/botRecentFiles'
  let {
    tab = $bindable(),
    botId,
    recent,
    onopen,
  }: {
    tab: 'files' | 'git'
    botId: string
    recent: ReturnType<typeof createRecentFiles>
    onopen: (file: RecentFile) => void
  } = $props()
</script>

<div class="mobile-bot-screen flex h-full flex-col">
  <div class="mx-auto mb-2 flex rounded-action bg-muted p-1">
    {#each ['files', 'git'] as const as key}<button
        class="min-h-11 rounded-chip px-5 aria-pressed:bg-card"
        aria-pressed={key === tab}
        onclick={() => (tab = key)}>{$t(key === 'files' ? 'files.title' : 'git.title')}</button
      >{/each}
  </div>
  {#if tab === 'git'}<BotGitTab {botId} />
  {:else if recent.current.length}<RecentFileList
      files={recent.current}
      {onopen}
      onclear={() => (recent.current = [])}
    />
  {:else}<p class="p-5 text-center text-muted-foreground">{$t('files.noneOpen')}</p>{/if}
</div>
