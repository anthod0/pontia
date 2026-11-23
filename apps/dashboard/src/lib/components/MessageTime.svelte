<script lang="ts">
  import { currentLocale } from '$lib/i18n';
  import { cn } from '$lib/utils';
  let { value, class: className }: { value: string; class?: string } = $props()

  const date = $derived(new Date(value))
  const label = $derived(
    date.toDateString() === new Date().toDateString()
      ? date.toLocaleTimeString($currentLocale, { hour: '2-digit', minute: '2-digit' })
      : date.toLocaleString($currentLocale, { month: 'short', day: 'numeric', hour: '2-digit', minute: '2-digit' }),
  )
</script>

<time datetime={value} title={date.toLocaleString($currentLocale)} class={cn('px-1 text-xs tabular-nums', className)}>{label}</time>
