<script lang="ts">
  import { t } from '$lib/i18n'
  import { toast } from 'svelte-sonner'
  import { useDeviceRuntime } from '../../device/context'
  import type { AgentProfileView } from '../../api/types'
  import * as AlertDialog from '$lib/components/ui/alert-dialog/index.js'

  import * as Sheet from '$lib/components/ui/sheet'
  import BottomSheet from '$lib/components/ui/overlay/BottomSheet.svelte'
  import { Button } from '$lib/components/ui/button'
  import { useMobileStack } from '$lib/mobile/overlayStack.svelte'

  const stack = useMobileStack()
  let { profile = $bindable(null), ondeleted }: { profile: AgentProfileView | null; ondeleted: (profile: AgentProfileView) => void } = $props()
  const device = useDeviceRuntime()
  const deleteProfile = device.agentProfileQueries.deleteAgentProfileMutation()

  async function remove(target: AgentProfileView | null) {
    if (!target) return
    profile = null
    try {
      await deleteProfile.mutateAsync(target.profile_id)
      ondeleted(target)
    } catch (cause) {
      toast.error(cause instanceof Error ? cause.message : String(cause))
    }
  }
</script>

{#if stack}
  <Sheet.Root open={!!profile} onOpenChange={(open) => { if (!open) profile = null }}>
    <BottomSheet dismiss={() => profile = null}>
      <Sheet.Title>{$t('profiles.deleteConfirm')}</Sheet.Title>
      <Sheet.Description>{$t('profiles.deleteDescription', { name: profile?.name ?? '' })}</Sheet.Description>
      <div class="flex justify-end gap-2 py-4">
        <Button variant="outline" onclick={() => stack.back()}>{$t('common.cancel')}</Button>
        <Button variant="destructive" onclick={() => { if (profile) { const target = profile; stack.back(() => void remove(target)) } }}>{$t('common.delete')}</Button>
      </div>
    </BottomSheet>
  </Sheet.Root>
{:else}
<AlertDialog.Root open={!!profile} onOpenChange={(open) => { if (!open) profile = null }}>
  <AlertDialog.Content>
    <AlertDialog.Header>
      <AlertDialog.Title>{$t('profiles.deleteConfirm')}</AlertDialog.Title>
      <AlertDialog.Description class="break-words">{$t('profiles.deleteDescription', { name: profile?.name ?? '' })}</AlertDialog.Description>
    </AlertDialog.Header>
    <AlertDialog.Footer>
      <AlertDialog.Cancel>{$t('common.cancel')}</AlertDialog.Cancel>
      <AlertDialog.Action variant="destructive" onclick={() => void remove(profile)}>{$t('common.delete')}</AlertDialog.Action>
    </AlertDialog.Footer>
  </AlertDialog.Content>
</AlertDialog.Root>
{/if}
