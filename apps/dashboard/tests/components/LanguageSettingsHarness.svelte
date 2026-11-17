<script lang="ts">
  import Layout from '../../src/routes/+layout.svelte';
  import AppearanceSettings from '../../src/components/settings/AppearanceSettings.svelte';
  import FileMentionEditor from '../../src/lib/components/file-picker/FileMentionEditor.svelte';
  import { t } from '../../src/lib/i18n';

  let draft = $state('');
</script>

<Layout>
  <AppearanceSettings />
  <FileMentionEditor bind:value={draft} placeholder={$t('chat.taskPlaceholder')} />
</Layout>
