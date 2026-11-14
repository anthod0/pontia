<script lang="ts">
  import { page } from '$app/state';
  import ErrorPage from '../pages/ErrorPage.svelte';
</script>

<ErrorPage status={page.status} message={page.error?.message} />
