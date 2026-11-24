import { requestPreviewToken, type PublicDevice } from "./remoteAccess";

export async function launchLocalhostPreview(
  device: Pick<PublicDevice, "handle" | "deviceId">,
  target: { port: number; redirect: string },
  signal: AbortSignal,
  loadingMessage: string,
  frame?: HTMLIFrameElement,
) {
  const name = `pontia-preview-${crypto.randomUUID()}`;
  const popup = frame ? null : window.open("", name);
  if (popup) {
    popup.opener = null;
    popup.document.body.textContent = loadingMessage;
  }
  try {
    const { origin, token } = await requestPreviewToken(device, target.port, signal);
    function submit() {
      if (signal.aborted || (frame && !frame.isConnected)) return;
      // The initial about:blank frame inherits the Dashboard origin for the exchange.
      const destination = frame ? frame.contentDocument! : document;
      const form = destination.createElement("form");
      form.method = "POST";
      form.action = origin + "/__pontia_auth";
      if (popup) form.target = name;
      else if (!frame) form.target = "_blank";
      if (!frame && !popup) form.rel = "noopener";
      form.hidden = true;
      for (const [key, value] of [
        ["token", token],
        ["redirect", target.redirect],
      ]) {
        const input = destination.createElement("input");
        input.type = "hidden";
        input.name = key;
        input.value = value;
        form.append(input);
      }
      destination.body.append(form);
      try {
        form.submit();
      } finally {
        form.remove();
      }
    }
    if (!frame && !popup) return submit;
    if (!popup?.closed) submit();
  } catch (error) {
    popup?.close();
    throw error;
  }
}
