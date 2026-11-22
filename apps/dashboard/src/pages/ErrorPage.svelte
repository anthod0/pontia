<script lang="ts">
  import { t } from '$lib/i18n';
  import { Button } from '$lib/components/ui/button/index.js'
  import * as Empty from '$lib/components/ui/empty/index.js'
  import { dashboardPath, navigate } from '$lib/navigation'
  import { returnToLastPage } from '$lib/lastPage'

  let { status, message }: { status: number; message?: string } = $props()

  const notFound = $derived(status === 404)

  function backToChat(event: MouseEvent): void {
    event.preventDefault()
    returnToLastPage()
    void navigate('/')
  }
</script>

<main class="flex min-h-svh p-6">
  <Empty.Root>
    <Empty.Header>
      <p class="text-sm font-medium text-muted-foreground">{status}</p>
      <Empty.Title class="text-2xl">{notFound ? $t('error.notFound') : $t('error.title')}</Empty.Title>
      <Empty.Description>
        {notFound ? $t('error.notFoundDescription') : message || $t('error.unexpected')}
      </Empty.Description>
    </Empty.Header>
    <Empty.Content>
      <Button href={dashboardPath('/')} onclick={backToChat}>{$t('nav.backToChat')}</Button>
    </Empty.Content>
  </Empty.Root>
</main>
