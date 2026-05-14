<script lang="ts">
  import { Badge } from '$lib/components/ui/badge/index.js'

  let { state }: { state: string | null | undefined } = $props()

  const variant = $derived<'default' | 'secondary' | 'destructive'>(
    state === 'failed' || state === 'cancelled' ? 'destructive' : state === 'running' || state === 'completed' ? 'default' : 'secondary',
  )
</script>

<Badge {variant}>{state ?? 'unknown'}</Badge>
