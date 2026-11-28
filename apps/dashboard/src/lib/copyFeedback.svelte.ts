import { onDestroy } from "svelte";
import { toast } from "svelte-sonner";
import { copyText } from "$lib/copyText";
import { isMobileShell } from "$lib/hooks/shellMode.svelte";
import { translate } from "$lib/i18n";

export function createCopyFeedback() {
  let confirmed = $state(false);
  let timer: ReturnType<typeof setTimeout> | undefined;
  onDestroy(() => clearTimeout(timer));

  async function copy(text: string | null | undefined): Promise<void> {
    if (!text || !(await copyText(text))) return;
    if (!isMobileShell()) {
      toast.success(translate("common.copied"));
      return;
    }
    confirmed = true;
    clearTimeout(timer);
    timer = setTimeout(() => (confirmed = false), 1600);
  }

  return {
    get confirmed() {
      return confirmed;
    },
    copy,
  };
}
