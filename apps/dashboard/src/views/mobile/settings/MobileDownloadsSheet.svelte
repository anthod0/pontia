<script lang="ts">
  import * as Sheet from '$lib/components/ui/sheet'
  import BottomSheet from '$lib/components/ui/overlay/BottomSheet.svelte'
  import { t } from '$lib/i18n'
  import DownloadList from '../../../components/layout/DownloadList.svelte'
  let { open = $bindable(false) }: { open: boolean } = $props()
</script>

<Sheet.Root bind:open>
  <BottomSheet dismiss={() => (open = false)} aria-describedby={undefined}>
    <Sheet.Title class="py-3">{$t('downloads.title')}</Sheet.Title>
    <DownloadList />
  </BottomSheet>
</Sheet.Root>
