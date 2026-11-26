import { CloudRequestError, requestPreviewToken, type PublicDevice } from "./remoteAccess";
import { isEdgePort } from "./edgeEndpoint";

export async function launchLocalhostPreview(
  device: Pick<PublicDevice, "handle" | "deviceId"> & { edgeApiOrigin?: string | null },
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
    const preview = await requestPreviewToken(device, target.port, signal);
    let origin: string;
    if (preview.mode === "edge_port") {
      const response = await fetch(`${preview.edgeApiOrigin}/__pontia/preview/open`, {
        method: "POST",
        headers: { Authorization: `Bearer ${preview.token}` },
        signal,
      });
      if (!response.ok) throw new CloudRequestError("Edge request failed.", response.status);
      let value: unknown;
      try {
        value = await response.json();
      } catch {
        throw new CloudRequestError("Edge returned an invalid preview response.");
      }
      const parsed =
        typeof value === "object" &&
        value !== null &&
        "preview_origin" in value &&
        typeof value.preview_origin === "string"
          ? URL.parse(value.preview_origin)
          : null;
      const control = new URL(preview.edgeApiOrigin);
      const listenPort = parsed ? Number(parsed.port || 443) : 0;
      if (
        !parsed ||
        parsed.origin !== (value as { preview_origin: string }).preview_origin ||
        parsed.protocol !== "https:" ||
        parsed.hostname !== control.hostname ||
        listenPort < 1024 ||
        !isEdgePort(listenPort) ||
        listenPort === Number(control.port || 443)
      ) {
        throw new CloudRequestError("Edge returned an invalid preview response.");
      }
      origin = parsed.origin;
    } else {
      origin = preview.origin;
    }
    const token = preview.token;
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
