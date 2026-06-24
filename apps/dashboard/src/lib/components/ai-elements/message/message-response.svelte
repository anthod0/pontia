<script lang="ts" module>
  import { tick } from 'svelte'
  import Markdown from 'svelte-exmarkdown'
  import { gfmPlugin } from 'svelte-exmarkdown/gfm'
  import rehypeHighlight from 'rehype-highlight'
  import { cn, type WithElementRef } from '$lib/utils.js'
  import { copyText } from '$lib/copyText'
  import type { HTMLAttributes } from 'svelte/elements'

  export interface MessageResponseProps extends WithElementRef<HTMLAttributes<HTMLDivElement>, HTMLDivElement> {
    content: string
    markdown?: boolean
  }

  const markdownPlugins = [
    gfmPlugin(),
    { rehypePlugin: rehypeHighlight },
  ]
</script>

<script lang="ts">
  let { content, markdown = false, class: className, ref = $bindable(null), ...restProps }: MessageResponseProps = $props()

  const codeCopyButtonClass = [
    'inline-flex',
    'items-center',
    'gap-1.5',
    'rounded-md',
    'px-2',
    'py-1',
    'text-xs',
    'font-medium',
    'text-muted-foreground',
    'transition',
    'hover:bg-background',
    'hover:text-foreground',
    'focus-visible:outline-none',
    'focus-visible:ring-2',
    'focus-visible:ring-ring',
  ].join(' ')

  const copyIconSvg = '<svg aria-hidden="true" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class="size-3.5"><rect width="14" height="14" x="8" y="8" rx="2" ry="2"></rect><path d="M4 16c-1.1 0-2-.9-2-2V4c0-1.1.9-2 2-2h10c1.1 0 2 .9 2 2"></path></svg>'
  const checkIconSvg = '<svg aria-hidden="true" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class="size-3.5"><path d="M20 6 9 17l-5-5"></path></svg>'

  $effect(() => {
    content
    markdown
    ref
    if (!markdown || !ref) return

    const root = ref
    void tick().then(() => addCodeCopyButtons(root))
  })

  function addCodeCopyButtons(root: HTMLDivElement): void {
    const codeBlocks = Array.from(root.querySelectorAll<HTMLElement>('pre > code'))

    for (const codeBlock of codeBlocks) {
      const pre = codeBlock.parentElement
      if (!pre || pre.dataset.codeBlockEnhanced === 'true') continue

      pre.dataset.codeBlockEnhanced = 'true'
      pre.classList.add('w-full', '!overflow-hidden', 'rounded-lg', 'border', 'border-border', 'bg-background')
      codeBlock.classList.add('block', 'min-w-full', '!bg-transparent', '!px-4', '!py-4', '!text-sm')

      const header = document.createElement('div')
      header.dataset.codeBlockHeader = 'true'
      header.className = 'flex items-center justify-between gap-3 px-3 py-1.5 text-xs text-muted-foreground'

      const language = document.createElement('span')
      language.className = 'truncate font-medium uppercase tracking-wide'
      language.textContent = languageLabel(codeBlock)

      const button = document.createElement('button')
      button.type = 'button'
      button.className = codeCopyButtonClass
      button.dataset.codeCopyButton = 'true'
      setCopyButtonState(button, false)

      const body = document.createElement('div')
      body.className = 'overflow-x-auto'
      pre.insertBefore(header, codeBlock)
      header.append(language, button)
      body.append(codeBlock)
      pre.append(body)
    }
  }

  async function handleResponseClick(event: MouseEvent): Promise<void> {
    const target = event.target instanceof Element ? event.target : null
    const button = target?.closest<HTMLButtonElement>('[data-code-copy-button]')
    if (!button) return

    const codeBlock = button.closest('pre')?.querySelector<HTMLElement>('code')
    const code = (codeBlock?.textContent ?? '').replace(/\n$/, '')
    const copied = await copyText(code)
    if (!copied) return

    setCopyButtonState(button, true)
    window.setTimeout(() => setCopyButtonState(button, false), 1600)
  }

  function setCopyButtonState(button: HTMLButtonElement, copied: boolean): void {
    button.innerHTML = `${copied ? checkIconSvg : copyIconSvg}<span>${copied ? 'Copied' : 'Copy'}</span>`
    button.setAttribute('aria-label', copied ? 'Code block copied' : 'Copy code block')
    button.title = copied ? 'Code block copied' : 'Copy code block'
  }

  function languageLabel(codeBlock: HTMLElement): string {
    const languageClass = Array.from(codeBlock.classList).find((className) => className.startsWith('language-'))
    const language = languageClass?.replace('language-', '').trim() || 'text'
    if (language === 'md') return 'markdown'
    if (language === 'py') return 'python'
    if (language === 'js') return 'javascript'
    if (language === 'ts') return 'ts'
    return language
  }
</script>

<div
  bind:this={ref}
  class={cn(
    'size-full min-w-0 [&>*:first-child]:mt-0 [&>*:last-child]:mb-0',
    markdown
      ? 'max-w-none overflow-x-auto text-base leading-7 [&_a]:text-primary [&_a]:underline [&_blockquote]:my-4 [&_blockquote]:border-l-2 [&_blockquote]:border-border [&_blockquote]:pl-3 [&_blockquote]:text-muted-foreground [&_code]:rounded [&_code]:bg-muted [&_code]:px-1 [&_code]:py-0.5 [&_em]:italic [&_h1]:mb-4 [&_h1]:mt-6 [&_h1]:text-2xl [&_h1]:font-bold [&_h2]:mb-3 [&_h2]:mt-5 [&_h2]:text-xl [&_h2]:font-semibold [&_h3]:mb-2 [&_h3]:mt-4 [&_h3]:text-lg [&_h3]:font-semibold [&_h4]:mb-2 [&_h4]:mt-3 [&_h4]:font-semibold [&_hr]:my-6 [&_hr]:border-border [&_li]:my-1 [&_ol]:my-3 [&_ol]:list-decimal [&_ol]:pl-5 [&_p]:my-3 [&_pre]:my-4 [&_pre]:overflow-x-auto [&_pre]:rounded-lg [&_strong]:font-semibold [&_table]:my-4 [&_table]:block [&_table]:max-w-full [&_table]:overflow-x-auto [&_table]:w-full [&_table]:border-collapse [&_table]:text-left [&_tbody_tr:last-child]:border-0 [&_td]:border [&_td]:border-border [&_td]:px-3 [&_td]:py-2 [&_th]:border [&_th]:border-border [&_th]:bg-muted [&_th]:px-3 [&_th]:py-2 [&_th]:font-semibold [&_thead_tr]:border-b [&_tr]:border-border [&_ul]:my-3 [&_ul]:list-disc [&_ul]:pl-5'
      : 'whitespace-pre-wrap',
    className,
  )}
  onclick={handleResponseClick}
  {...restProps}
>
  {#if markdown}
    <Markdown md={content} plugins={markdownPlugins} />
  {:else}
    {content}
  {/if}
</div>
