<script lang="ts">
  import { CircleAlert } from '@lucide/svelte'
  import * as Alert from '$lib/components/ui/alert/index.js'
  import * as Card from '$lib/components/ui/card/index.js'
  import * as Empty from '$lib/components/ui/empty/index.js'
  import TaskPageFrame from './TaskPageFrame.svelte'
</script>

<TaskPageFrame title="Artifacts" description="Task artifact discovery, list, and content viewer are temporarily disabled.">
  <Card.Root>
    <Card.Header>
      <Card.Title>Artifacts temporarily disabled</Card.Title>
      <Card.Description>The dashboard will not request artifact endpoints until the artifact model is implemented.</Card.Description>
    </Card.Header>
    <Card.Content class="space-y-4">
      <Alert.Root>
        <CircleAlert class="size-4" />
        <Alert.Title>Artifacts are temporarily disabled</Alert.Title>
        <Alert.Description>Artifact discovery and content viewing are hidden for now, so this page does not call `/external/v1/sessions/:id/artifacts`.</Alert.Description>
      </Alert.Root>
      <Empty.Root>
        <Empty.Header>
          <Empty.Title>No artifact data loaded</Empty.Title>
          <Empty.Description>Re-enable this page after artifact registration and display semantics are finalized.</Empty.Description>
        </Empty.Header>
      </Empty.Root>
    </Card.Content>
  </Card.Root>
</TaskPageFrame>
