<script lang="ts">
  import { SvelteMap } from 'svelte/reactivity'
  import CaretRightIcon from 'phosphor-svelte/lib/CaretRightIcon'
  import { Button } from '$lib/components/ui/button'
  import * as Alert from '$lib/components/ui/alert'
  import { t } from '$lib/i18n'
  import { parseStructuredPreview, type TreeNode } from '../../features/chat/structuredPreview'

  let { source, format }: { source: string; format: 'json' | 'yaml' } = $props()
  const root = $derived.by(() => {
    try {
      return parseStructuredPreview(source, format)
    } catch {
      return null
    }
  })
  let expandAll = $state<boolean | null>(null)
  const overrides = new SvelteMap<TreeNode, boolean>()
  function setAll(open: boolean) {
    overrides.clear()
    expandAll = open
  }

  const colors = {
    string: 'text-emerald-800 dark:text-emerald-300',
    number: 'text-blue-800 dark:text-blue-300',
    boolean: 'text-violet-800 dark:text-violet-300',
    null: 'text-muted-foreground',
    alias: 'text-muted-foreground',
  }
</script>

{#snippet row(node: TreeNode, label?: string)}
  {#snippet key()}
    {#if label !== undefined}<span class="font-medium break-all">{label || '""'}</span>{/if}
  {/snippet}
  {#if 'children' in node}
    {@const count = node.children.length}
    {@const summary = node.kind === 'array' ? '[' + count + ']' : '{' + count + '}'}
    {@const open = overrides.get(node) ?? expandAll ?? (node === root)}
    {#if count}
      <button type="button" aria-expanded={open} class="flex min-h-8 max-w-full items-center gap-1.5 rounded-tooltip px-1 text-left hover:bg-muted focus-visible:outline-primary" onclick={() => overrides.set(node, !open)}>
        <CaretRightIcon class={['size-3 shrink-0 text-muted-foreground', open && 'rotate-90']} />
        {@render key()}<span class="text-muted-foreground">{summary}</span>
      </button>
      {#if open}
        <ul class="ms-2 border-s ps-2">
          {#each node.children as [key, child]}
            <li>{@render row(child, key)}</li>
          {/each}
        </ul>
      {/if}
    {:else}
      <div class="py-1 ps-5">{@render key()} <span class="text-muted-foreground">{summary}</span></div>
    {/if}
  {:else}
    <div class="py-1 ps-5 break-all">{@render key()}{#if label !== undefined}<span class="text-muted-foreground">: </span>{/if}<span class={colors[node.kind]}>{node.text}</span></div>
  {/if}
{/snippet}

{#if root}
  {#if 'children' in root && root.children.length}
    <div class="mb-2 flex flex-wrap gap-1 border-b pb-2">
      <Button variant="ghost" size="sm" onclick={() => setAll(true)}>{$t('files.expandAll')}</Button>
      <Button variant="ghost" size="sm" onclick={() => setAll(false)}>{$t('files.collapseAll')}</Button>
    </div>
  {/if}
  <div class="font-mono text-xs leading-relaxed">{@render row(root)}</div>
{:else}
  <Alert.Root><Alert.Description>{$t('files.treeParseError', {format: format.toUpperCase()})}</Alert.Description></Alert.Root>
{/if}
