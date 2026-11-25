<script lang="ts">
  import BotTodoDirectory from '../../components/bots/BotTodoDirectory.svelte'
  import { useMobileDirectory } from '../../features/bots/mobileDirectory'
  const directory = useMobileDirectory()!
</script>

<BotTodoDirectory {directory} />
