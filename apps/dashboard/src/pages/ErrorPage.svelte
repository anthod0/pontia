<script lang="ts">
  import { t } from '$lib/i18n';
  import { Button } from '$lib/components/ui/button/index.js'
  import * as Empty from '$lib/components/ui/empty/index.js'
  import { dashboardPath } from '$lib/dashboardRoutes'

  let { status, message }: { status: number; message?: string } = $props()

  const notFound = $derived(status === 404)
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
      <Button href={dashboardPath('/')}>{$t('nav.backToChat')}</Button>
    </Empty.Content>
  </Empty.Root>
</main>
