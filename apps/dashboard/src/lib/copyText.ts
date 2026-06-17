export async function copyText(text: string): Promise<boolean> {
  try {
    if (navigator.clipboard?.writeText) {
      await navigator.clipboard.writeText(text);
      return true;
    }
  } catch {
    // Fall through to the legacy copy path for local HTTP origins where the
    // async Clipboard API can exist but reject because the context is insecure.
  }

  return copyTextWithExecCommand(text);
}

function copyTextWithExecCommand(text: string): boolean {
  if (typeof document.execCommand !== 'function') return false;

  const textarea = document.createElement('textarea');
  textarea.value = text;
  textarea.setAttribute('readonly', '');
  textarea.style.position = 'fixed';
  textarea.style.left = '-9999px';
  textarea.style.top = '0';
  document.body.appendChild(textarea);

  const selection = document.getSelection();
  const previousRange = selection?.rangeCount ? selection.getRangeAt(0) : null;

  textarea.select();
  textarea.setSelectionRange(0, textarea.value.length);

  try {
    return document.execCommand('copy');
  } finally {
    document.body.removeChild(textarea);
    if (selection && previousRange) {
      selection.removeAllRanges();
      selection.addRange(previousRange);
    }
  }
}
