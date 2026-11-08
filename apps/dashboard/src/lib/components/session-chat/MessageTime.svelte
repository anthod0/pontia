<script lang="ts">
  let { value }: { value: string } = $props()

  const date = $derived(new Date(value))
  const label = $derived(
    date.toDateString() === new Date().toDateString()
      ? date.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' })
      : date.toLocaleString([], { month: 'short', day: 'numeric', hour: '2-digit', minute: '2-digit' }),
  )
</script>

<time datetime={value} title={date.toLocaleString()} class="px-1 text-xs tabular-nums">{label}</time>
