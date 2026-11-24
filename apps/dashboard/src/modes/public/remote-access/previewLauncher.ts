import { requestPreviewToken, type PublicDevice } from "./remoteAccess";

export async function launchLocalhostPreview(
  device: Pick<PublicDevice, "handle" | "deviceId">,
  target: { port: number; redirect: string },
  signal: AbortSignal,
  loadingMessage: string,
) {
  const name = `pontia-preview-${crypto.randomUUID()}`;
  const popup = window.open("", name);
  if (popup) {
    popup.opener = null;
    popup.document.body.textContent = loadingMessage;
  }
  try {
    const { origin, token } = await requestPreviewToken(device, target.port, signal);
    function submit() {
      if (signal.aborted) return;
      const form = document.createElement("form");
      form.method = "POST";
      form.action = origin + "/__pontia_auth";
      form.target = popup ? name : "_blank";
      if (!popup) form.rel = "noopener";
      form.hidden = true;
      for (const [key, value] of [
        ["token", token],
        ["redirect", target.redirect],
      ]) {
        const input = document.createElement("input");
        input.type = "hidden";
        input.name = key;
        input.value = value;
        form.append(input);
      }
      document.body.append(form);
      try {
        form.submit();
      } finally {
        form.remove();
      }
    }
    if (!popup) return submit;
    if (!popup.closed) submit();
  } catch (error) {
    popup?.close();
    throw error;
  }
}
