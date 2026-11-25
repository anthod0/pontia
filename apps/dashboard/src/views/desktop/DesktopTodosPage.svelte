<script lang="ts">
  import { t } from '$lib/i18n'
  import BotTodoDirectory from '../../components/bots/BotTodoDirectory.svelte'
  import { createBotDirectory } from '../../features/bots/botDirectory.svelte'
  import { useDeviceRuntime } from '../../device/context'

  const device = useDeviceRuntime()
  const directory = createBotDirectory(device)
</script>

<section class="mx-auto grid max-w-4xl gap-5">
  <h1 class="text-2xl font-semibold">{$t('bots.todos')}</h1>
  <BotTodoDirectory {directory} />
</section>
