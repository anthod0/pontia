<script lang="ts">
  import { provideFilePreviewNavigator, type FilePreviewTarget } from '../../src/components/chat/filePreviewContext'
  import { provideDeviceRuntime } from '../../src/device/context'
  import { provideMobileStack } from '../../src/lib/mobile/overlayStack.svelte'
  import type { SessionChatThoughtStep } from '../../src/lib/session-chat/sessionChat'
  import type { SessionView } from '../../src/api/types'
  import MobileSessionStepsCard from '../../src/views/mobile/sessions/MobileSessionStepsCard.svelte'
  import { testDevice } from '../deviceFixture'

  let {
    steps,
    session,
    onOpen,
  }: { steps: SessionChatThoughtStep[]; session: SessionView; onOpen: (target: FilePreviewTarget) => void } = $props()

  provideDeviceRuntime(testDevice)
  provideMobileStack()
  provideFilePreviewNavigator({ open: (target) => onOpen(target) })
</script>

<MobileSessionStepsCard {steps} active={false} {session} />
