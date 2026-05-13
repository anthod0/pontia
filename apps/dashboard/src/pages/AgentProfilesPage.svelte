<script lang="ts">
  import PlaceholderPage from './PlaceholderPage.svelte'
</script>

<PlaceholderPage
  title="Agent Profiles"
  description="Inspect available execution profiles and capabilities. Editing can be added when the External API supports it."
  details={['Client differences should be represented through capabilities.', 'Profile data will come from /external/v1/agent-profiles.']}
/>
