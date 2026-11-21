<script lang="ts">
  import { t } from '$lib/i18n'
  import { toast } from 'svelte-sonner'
  import { useDeviceRuntime } from '../../device/context'
  import type { AgentProfileView } from '../../api/types'
  import * as AlertDialog from '$lib/components/ui/alert-dialog/index.js'

  let { profile = $bindable(null), ondeleted }: { profile: AgentProfileView | null; ondeleted: (profile: AgentProfileView) => void } = $props()
  const device = useDeviceRuntime()
  const deleteProfile = device.agentProfileQueries.deleteAgentProfileMutation()

  async function remove() {
    const target = profile
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

<AlertDialog.Root open={!!profile} onOpenChange={(open) => { if (!open) profile = null }}>
  <AlertDialog.Content>
    <AlertDialog.Header>
      <AlertDialog.Title>{$t('profiles.deleteConfirm')}</AlertDialog.Title>
      <AlertDialog.Description class="break-words">{$t('profiles.deleteDescription', { name: profile?.name ?? '' })}</AlertDialog.Description>
    </AlertDialog.Header>
    <AlertDialog.Footer>
      <AlertDialog.Cancel>{$t('common.cancel')}</AlertDialog.Cancel>
      <AlertDialog.Action variant="destructive" onclick={remove}>{$t('common.delete')}</AlertDialog.Action>
    </AlertDialog.Footer>
  </AlertDialog.Content>
</AlertDialog.Root>
