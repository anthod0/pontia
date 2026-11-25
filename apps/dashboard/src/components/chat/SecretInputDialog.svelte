<script lang="ts">
  import { tick } from 'svelte'
  import * as Dialog from '$lib/components/ui/dialog'
  import ResponsiveDialog from '$lib/components/ui/overlay/ResponsiveDialog.svelte'
  import { Button } from '$lib/components/ui/button'
  import { Input } from '$lib/components/ui/input'
  import { Textarea } from '$lib/components/ui/textarea'
  import { Label } from '$lib/components/ui/label'
  import { t } from '$lib/i18n'
  import { hasControl, secretSizeValid } from '$lib/uploads/secretMask'

  let {open = $bindable(false), onConfirm, onClose}: {
    open: boolean
    onConfirm: (input: {value: string; label?: string}) => void
    onClose: () => void
  } = $props()
  const id = $props.id()
  let value = $state('')
  let label = $state('')
  let multiline = $state(false)
  let visible = $state(false)
  let field = $state<HTMLTextAreaElement | null>(null)
  const protections = {autocomplete:'new-password', spellcheck:false, autocorrect:'off', autocapitalize:'off'} as const
  const validValue = $derived(secretSizeValid(value))
  const validLabel = $derived(Array.from(label.trim()).length <= 64 && !hasControl(label))

  function close() {
    value = ''
    label = ''
    visible = false
    multiline = false
    open = false
  }
  function confirm(event: SubmitEvent) {
    event.preventDefault()
    if (!validValue || !validLabel) return
    onConfirm({value,label:label.trim() || undefined})
    close()
  }
  // Textarea selection offsets count CRLF as one character.
  function rawOffset(offset: number) {
    let raw = 0
    for (let i = 0; i < offset; i++) {
      raw += value.startsWith('\r\n', raw) ? 2 : 1
    }
    return raw
  }
  function editMultiline(input: HTMLTextAreaElement) {
    const previous = value.replace(/\r\n?/g, '\n')
    const next = input.value
    let start = 0
    while (start < input.selectionStart && previous[start] === next[start]) start++
    let oldEnd = previous.length
    let newEnd = next.length
    while (oldEnd > start && newEnd > start && previous[oldEnd - 1] === next[newEnd - 1]) {
      oldEnd--
      newEnd--
    }
    value = value.slice(0, rawOffset(start)) + next.slice(start, newEnd) + value.slice(rawOffset(oldEnd))
  }
  function paste(event: ClipboardEvent) {
    const text = event.clipboardData?.getData('text') ?? ''
    if (!/[\r\n]/.test(text)) return
    event.preventDefault()
    const input = event.currentTarget as HTMLInputElement
    const start = input.selectionStart ?? 0
    value = value.slice(0,rawOffset(start)) + text + value.slice(rawOffset(input.selectionEnd ?? start))
    multiline = true
    const caret = start + text.replace(/\r\n/g, "\n").length
    void tick().then(() => {field?.focus(); field?.setSelectionRange(caret, caret)})
  }
</script>
<Dialog.Root bind:open onOpenChange={next => {if (!next) close()}}>
  <ResponsiveDialog mode="sheet" dismiss={close} onCloseAutoFocus={event => {event.preventDefault(); onClose()}}>
    <Dialog.Header><Dialog.Title>{$t('secrets.add')}</Dialog.Title><Dialog.Description>{$t('secrets.description')}</Dialog.Description></Dialog.Header>
    <form onsubmit={confirm} class="grid gap-4 py-3">
      <div class="grid gap-2">
        <Label for={id}>{$t('secrets.secret')}</Label>
        {#if multiline}
          <Textarea bind:ref={field} id={id} value={value} oninput={e => editMultiline(e.currentTarget)} onpaste={paste} {...protections} class={['max-h-36 resize-none overflow-auto', !visible && '[-webkit-text-security:disc]']} />
        {:else}
          <Input id={id} type={visible ? 'text' : 'password'} bind:value onpaste={paste} {...protections}/>
        {/if}
        <div class="flex gap-2">
          <Button type="button" size="sm" variant="outline" aria-pressed={visible} onclick={() => visible = !visible}>{visible ? $t('secrets.hide') : $t('secrets.show')}</Button>
          <Button type="button" size="sm" variant="outline" aria-pressed={multiline} disabled={multiline && /[\r\n]/.test(value)} onclick={() => multiline = !multiline}>{$t('secrets.multiline')}</Button>
        </div>
        {#if value && !validValue}<p class="text-sm text-destructive">{$t('secrets.sizeLimit')}</p>{/if}
      </div>
      <div class="grid gap-2"><Label for={id+'-label'}>{$t('secrets.label')}</Label><Input id={id+'-label'} bind:value={label} autocomplete="off"/>
        {#if !validLabel}<p class="text-sm text-destructive">{$t('secrets.labelLimit')}</p>{/if}
      </div>
      <Dialog.Footer><Button type="button" variant="outline" onclick={close}>{$t('common.cancel')}</Button><Button type="submit" disabled={!validValue || !validLabel}>{$t('common.add')}</Button></Dialog.Footer>
    </form>
  </ResponsiveDialog>
</Dialog.Root>
