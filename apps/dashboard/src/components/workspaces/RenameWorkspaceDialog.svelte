<script lang="ts">
  import ResponsiveDialog from '$lib/components/ui/overlay/ResponsiveDialog.svelte'
  import { t } from '$lib/i18n'
  import { Button } from '$lib/components/ui/button/index.js'
  import * as Dialog from '$lib/components/ui/dialog/index.js'
  import { Input } from '$lib/components/ui/input/index.js'
  import { Label } from '$lib/components/ui/label/index.js'
  import type { createWorkspaceBrowser } from '../../features/settings/workspaceBrowser.svelte'
  let { browser }: { browser: ReturnType<typeof createWorkspaceBrowser> } = $props()
</script>

<Dialog.Root bind:open={browser.renameWorkspaceDialogOpen}>
  {#if browser.renamingWorkspace}
    <ResponsiveDialog mode="sheet" dismiss={browser.cancelRenamingWorkspace} class="sm:max-w-md">
      <form
        onsubmit={(event) => {
          event.preventDefault()
          void browser.confirmRenameWorkspace()
        }}
      >
        <Dialog.Header>
          <Dialog.Title>{$t('workspaces.confirmRename')}</Dialog.Title>
          <Dialog.Description
            >{$t('common.rename')}
            <span class="font-medium text-foreground"
              >{browser.renamingWorkspace.name ?? browser.renamingWorkspace.display_path}</span
            >.</Dialog.Description
          >
        </Dialog.Header>
        <div class="mt-4 space-y-2">
          <Label for="rename-workspace-name">{$t('workspaces.displayName')}</Label>
          <Input
            id="rename-workspace-name"
            bind:value={browser.renamingWorkspaceName}
            placeholder={browser.renamingWorkspace.display_path}
          />
          <p class="text-xs text-muted-foreground">{$t('workspaces.clearNameHint')}</p>
          {#if browser.renameError}<p class="text-xs text-destructive" role="alert">{browser.renameError}</p>{/if}
        </div>
        <Dialog.Footer class="mt-5">
          <Button
            type="button"
            variant="outline"
            onclick={browser.cancelRenamingWorkspace}
            disabled={browser.savingRename}>{$t('common.cancel')}</Button
          >
          <Button type="submit" disabled={browser.savingRename}
            >{browser.savingRename ? $t('common.saving') : $t('workspaces.rename')}</Button
          >
        </Dialog.Footer>
      </form>
    </ResponsiveDialog>
  {/if}
</Dialog.Root>
