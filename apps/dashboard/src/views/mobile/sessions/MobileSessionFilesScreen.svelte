<script lang="ts">
  import { t } from '$lib/i18n'
  import {
    sessionRecentFiles,
    withoutSessionRecentFiles,
    type createSessionRecentFiles,
  } from '$lib/sessionRecentFiles'
  import RecentFileList from '../../../components/chat/files/RecentFileList.svelte'
  import type { RecentFile } from '$lib/botRecentFiles'
  let {
    sessionId,
    recent,
    onopen,
  }: {
    sessionId: string
    recent: ReturnType<typeof createSessionRecentFiles>
    onopen: (file: RecentFile) => void
  } = $props()
  const files = $derived(sessionRecentFiles(recent.current, sessionId))
</script>

{#if files.length}<RecentFileList
    {files}
    {onopen}
    onclear={() => (recent.current = withoutSessionRecentFiles(recent.current, sessionId))}
  />
{:else}<p class="p-6 text-center text-muted-foreground">{$t('files.noneOpenDescription')}</p>{/if}
