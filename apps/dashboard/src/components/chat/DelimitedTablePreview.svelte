<script lang="ts">
  import * as Table from '$lib/components/ui/table'
  import { currentLocale, t } from '$lib/i18n'
  import { MAX_COLUMNS, parseDelimitedTable } from '../../features/chat/delimitedTable'
  import ArrowsDownUpIcon from 'phosphor-svelte/lib/ArrowsDownUpIcon'
  import ArrowDownIcon from 'phosphor-svelte/lib/ArrowDownIcon'
  import ArrowUpIcon from 'phosphor-svelte/lib/ArrowUpIcon'

  let { text, delimiter }: { text: string; delimiter: ',' | '\t' } = $props()
  const table = $derived(parseDelimitedTable(text, delimiter))
  const columns = $derived(Math.min(table.columnCount, MAX_COLUMNS))
  const collator = $derived(new Intl.Collator($currentLocale, { numeric: true, sensitivity: 'base' }))
  let sort = $state<{ column: number; direction: 'ascending' | 'descending' } | null>(null)

  function sortBy(column: number) {
    if (sort?.column !== column) sort = { column, direction: 'ascending' }
    else if (sort.direction === 'ascending') sort = { column, direction: 'descending' }
    else sort = null
  }

  function compare(a: string, b: string) {
    const x = a.trim() === '' ? NaN : Number(a)
    const y = b.trim() === '' ? NaN : Number(b)
    if (Number.isFinite(x) && Number.isFinite(y)) return x - y
    if (Number.isFinite(x)) return -1
    if (Number.isFinite(y)) return 1
    return collator.compare(a, b)
  }

  const rows = $derived.by(() => {
    const order = sort
    if (!order) return table.rows
    return [...table.rows].sort((a, b) =>
      compare(a[order.column] ?? '', b[order.column] ?? '') * (order.direction === 'ascending' ? 1 : -1))
  })
</script>

<p class="px-3 py-2 text-xs text-muted-foreground">
  {$t('files.tableSize', { rows: table.rowCount, columns: table.columnCount })}
  {#if rows.length < table.rowCount || columns < table.columnCount}
    {$t('files.tableLimit', { rows: rows.length, columns })}
  {/if}
</p>
<div class="[&>[data-slot=table-container]]:overflow-visible">
  <Table.Root class="w-max min-w-full">
    <Table.Header>
      <Table.Row>
        {#each Array(columns) as _, column (column)}
          <Table.Head scope="col" aria-sort={sort?.column === column ? sort.direction : undefined} class="sticky top-0 z-10 bg-muted px-0">
            <button class="flex min-h-11 w-full items-center gap-2 px-3 text-left focus-visible:outline-ring" onclick={() => sortBy(column)}>
              <span class="max-w-64 whitespace-pre-wrap wrap-anywhere">{table.header[column] || $t('files.tableColumn', {column: column + 1})}</span>
              {#if sort?.column !== column}<ArrowsDownUpIcon class="size-3.5 shrink-0 text-muted-foreground" />
              {:else if sort.direction === 'ascending'}<ArrowUpIcon class="size-3.5 shrink-0" />
              {:else}<ArrowDownIcon class="size-3.5 shrink-0" />{/if}
            </button>
          </Table.Head>
        {/each}
      </Table.Row>
    </Table.Header>
    <Table.Body>
      {#each rows as row (row)}
        <Table.Row>
          {#each Array(columns) as _, column (column)}
            <Table.Cell class="max-w-80 px-3 align-top whitespace-pre-wrap wrap-anywhere">{row[column] ?? ''}</Table.Cell>
          {/each}
        </Table.Row>
      {/each}
    </Table.Body>
  </Table.Root>
</div>
