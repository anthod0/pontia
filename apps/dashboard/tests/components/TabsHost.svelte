<script lang="ts">
  import * as Tabs from '../../src/lib/components/ui/tabs/index.js'
</script>

<Tabs.Root value="active">
  <Tabs.List>
    <Tabs.Trigger value="active">Active</Tabs.Trigger>
    <Tabs.Trigger value="exited">Exited</Tabs.Trigger>
  </Tabs.List>
</Tabs.Root>
