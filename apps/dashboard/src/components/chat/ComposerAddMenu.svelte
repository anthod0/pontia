<script lang="ts">
  import ResponsiveMenu from '$lib/components/ui/overlay/ResponsiveMenu.svelte'
  import * as DropdownMenu from '$lib/components/ui/dropdown-menu'
  import { Button } from '$lib/components/ui/button'
  import { t } from '$lib/i18n'
  import PlusIcon from 'phosphor-svelte/lib/PlusIcon'
  import FileIcon from 'phosphor-svelte/lib/FileIcon'
  import KeyIcon from 'phosphor-svelte/lib/KeyIcon'
  import SecretInputDialog from './SecretInputDialog.svelte'

  let {disabled, onSelectFile, onConfirm, onClose, mobile = false}: {
    disabled: boolean
    onSelectFile: () => void
    onConfirm: (input: {value: string; label?: string}) => void
    onClose: () => void
    mobile?: boolean
  } = $props()
  let triggerButton = $state<HTMLButtonElement | null>(null)
  let secretOpen = $state(false)
</script>
<ResponsiveMenu keepClearOf={triggerButton} label={$t('common.add')} side="top" align="start" onCloseAutoFocus={event => {if (secretOpen) event.preventDefault()}}>
  {#snippet trigger()}
    <DropdownMenu.Trigger bind:ref={triggerButton} {disabled}>
      {#snippet child({props})}
        <Button {...props} type="button" variant="ghost" size="icon-sm" class={mobile ? 'mobile-icon shrink-0' : ''} aria-label={$t('common.add')} title={$t('common.add')} {disabled}><PlusIcon class="size-4"/></Button>
      {/snippet}
    </DropdownMenu.Trigger>
  {/snippet}
  <DropdownMenu.Item onSelect={onSelectFile}><FileIcon/>{$t('secrets.file')}</DropdownMenu.Item>
  <DropdownMenu.Item onSelect={() => secretOpen = true}><KeyIcon/>{$t('secrets.secret')}</DropdownMenu.Item>
</ResponsiveMenu>
<SecretInputDialog bind:open={secretOpen} {onConfirm} {onClose}/>
