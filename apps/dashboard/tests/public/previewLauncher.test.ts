import { render, screen, waitFor } from "@testing-library/svelte";
import { afterEach, expect, test, vi } from "vitest";
import MessageResponse from "../../src/lib/components/ai-elements/message/message-response.svelte";
import { deviceContext } from "../../src/device/context";
import { launchLocalhostPreview } from "../../src/modes/public/remote-access/previewLauncher";
import { requestPreviewToken } from "../../src/modes/public/remote-access/remoteAccess";

const device = { handle: "studio", deviceId: "01234567-89ab-cdef-0123-456789abcdef" };
const edgeDevice = { ...device, edgeApiOrigin: "https://8.8.8.8:8443" };
const href = "http://localhost:5173/page?q=1#part";
const origin = "https://5173-0123456789abcdef0123456789abcdef.private-edge.pontia.run:8443";
const response = { preview_origin: origin, token: "secret", expires_at: 1000 };
const signal = new AbortController().signal;
afterEach(() => {
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
});

test("public Markdown link opens a tab before requesting a ticket and posts to that tab", async () => {
  const popup = { opener: window, document: document.implementation.createHTMLDocument() };
  const open = vi.spyOn(window, "open").mockReturnValue(popup as unknown as Window);
  const fetchToken = vi.fn(() => {
    expect(open).toHaveBeenCalledTimes(1);
    expect(popup.opener).toBeNull();
    return Promise.resolve(Response.json(response));
  });
  vi.stubGlobal("fetch", fetchToken);
  const submit = vi.spyOn(HTMLFormElement.prototype, "submit").mockImplementation(function (
    this: HTMLFormElement,
  ) {
    expect(this.method).toBe("post");
    expect(this.target).toBe(open.mock.calls[0][1]);
    expect(this.action).toBe(origin + "/__pontia_auth");
    expect(Object.fromEntries(new FormData(this))).toEqual({
      token: "secret",
      redirect: "/page?q=1#part",
    });
  });
  const access = {
    openLocalhostPreview: (target: { port: number; redirect: string }) =>
      launchLocalhostPreview(device, target, signal, "Loading"),
  };
  render(MessageResponse, {
    props: { content: `[preview](${href})`, markdown: true },
    context: new Map([[deviceContext, { access }]]),
  });
  const link = screen.getByRole("link");
  expect(link).toHaveAttribute("href", href);
  expect(link.dispatchEvent(new MouseEvent("click", { bubbles: true, cancelable: true }))).toBe(
    false,
  );
  expect(open).toHaveBeenCalledOnce();
  await waitFor(() => expect(submit).toHaveBeenCalledOnce());
  expect(fetchToken).toHaveBeenCalledWith(
    "https://pontia.dev/api/dashboard/devices/studio/preview-token",
    expect.objectContaining({
      method: "POST",
      credentials: "include",
      body: JSON.stringify({ port: 5173 }),
    }),
  );
  expect(document.querySelector("form")).toBeNull();
});

test.each([true, false])(
  "posts the ticket only while the preview frame is mounted (%s)",
  async (mounted) => {
    const frame = document.createElement("iframe");
    document.body.append(frame);
    const open = vi.spyOn(window, "open");
    vi.stubGlobal("fetch", vi.fn().mockResolvedValue(Response.json(response)));
    const post = vi
      .spyOn(
        Object.getPrototypeOf(frame.contentDocument!.createElement("form")) as HTMLFormElement,
        "submit",
      )
      .mockImplementation(function (this: HTMLFormElement) {
        expect(this.target).toBe("");
        expect(this.ownerDocument).toBe(frame.contentDocument);
        expect(this.action).toBe(origin + "/__pontia_auth");
        expect(Object.fromEntries(new FormData(this))).toEqual({
          token: "secret",
          redirect: "/app",
        });
      });
    try {
      const launch = launchLocalhostPreview(
        device,
        { port: 5173, redirect: "/app" },
        signal,
        "Loading",
        frame,
      );
      if (!mounted) frame.remove();
      expect(await launch).toBeUndefined();
      expect(open).not.toHaveBeenCalled();
      expect(post).toHaveBeenCalledTimes(mounted ? 1 : 0);
    } finally {
      frame.remove();
    }
  },
);

test("a blocked popup provides a second user gesture that submits without another request", async () => {
  vi.spyOn(window, "open").mockReturnValue(null);
  vi.stubGlobal("fetch", vi.fn().mockResolvedValue(Response.json(response)));
  const post = vi.spyOn(HTMLFormElement.prototype, "submit").mockImplementation(function (
    this: HTMLFormElement,
  ) {
    expect(this.target).toBe("_blank");
    expect(this.rel).toBe("noopener");
  });
  const retry = await launchLocalhostPreview(
    device,
    { port: 5173, redirect: "/" },
    signal,
    "Loading",
  );
  expect(post).not.toHaveBeenCalled();
  retry!();
  expect(post).toHaveBeenCalledOnce();
  expect(fetch).toHaveBeenCalledTimes(1);
});

test.each([
  { preview_origin: origin.replace(":8443", ":0") },
  { preview_origin: origin.replace("5173", "5174") },
  { preview_origin: origin.replace("0123", "9876") },
  { preview_origin: origin.replace("https:", "http:") },
  { preview_origin: origin + "/page" },
  { preview_origin: origin + "?query" },
  { preview_origin: origin + "#fragment" },
  { preview_origin: origin.replace("private-edge", "nested.edge") },
  { preview_origin: origin.replace("pontia.run", "pontia.run.example") },
  { preview_origin: origin.replace("//", "//user@") },
  { token: "" },
  { expires_at: 1.5 },
])("rejects an invalid Cloud preview response %j", async (invalid) => {
  vi.stubGlobal("fetch", vi.fn().mockResolvedValue(Response.json({ ...response, ...invalid })));
  await expect(requestPreviewToken(device, 5173, signal)).rejects.toThrow();
});

test("opens an IP edge listener before posting the preview token", async () => {
  const popup = { opener: window, document: document.implementation.createHTMLDocument() };
  vi.spyOn(window, "open").mockReturnValue(popup as unknown as Window);
  const fetcher = vi.fn(async (input: string | URL | Request, init?: RequestInit) => {
    const url = input.toString();
    if (url.endsWith("/preview-token")) {
      return Response.json({
        mode: "edge_port",
        edge_api_origin: edgeDevice.edgeApiOrigin,
        token: "edge-secret",
        expires_at: 1000,
      });
    }
    expect(url).toBe("https://8.8.8.8:8443/__pontia/preview/open");
    expect(init).toMatchObject({
      method: "POST",
      headers: { Authorization: "Bearer edge-secret" },
    });
    expect(init).not.toHaveProperty("credentials");
    return Response.json({
      preview_origin: "https://8.8.8.8:20000",
      listen_port: 20000,
      target_port: 5173,
      remapped: true,
    });
  });
  vi.stubGlobal("fetch", fetcher);
  const submit = vi.spyOn(HTMLFormElement.prototype, "submit").mockImplementation(function (
    this: HTMLFormElement,
  ) {
    expect(this.action).toBe("https://8.8.8.8:20000/__pontia_auth");
    expect(Object.fromEntries(new FormData(this))).toEqual({
      token: "edge-secret",
      redirect: "/app",
    });
  });

  await launchLocalhostPreview(edgeDevice, { port: 5173, redirect: "/app" }, signal, "Loading");

  expect(fetcher).toHaveBeenCalledTimes(2);
  expect(submit).toHaveBeenCalledOnce();
});

test.each([
  "https://9.9.9.9:20000",
  "https://8.8.8.8:8443",
  "https://8.8.8.8:443",
  "https://8.8.8.8:10080",
  "http://8.8.8.8:20000",
  "https://8.8.8.8:20000/path",
])("rejects an unsafe edge preview origin %s", async (previewOrigin) => {
  const popup = {
    opener: window,
    close: vi.fn(),
    document: document.implementation.createHTMLDocument(),
  };
  vi.spyOn(window, "open").mockReturnValue(popup as unknown as Window);
  vi.stubGlobal(
    "fetch",
    vi
      .fn()
      .mockResolvedValueOnce(
        Response.json({
          mode: "edge_port",
          edge_api_origin: edgeDevice.edgeApiOrigin,
          token: "edge-secret",
          expires_at: 1000,
        }),
      )
      .mockResolvedValueOnce(Response.json({ preview_origin: previewOrigin })),
  );

  await expect(
    launchLocalhostPreview(edgeDevice, { port: 5173, redirect: "/" }, signal, "Loading"),
  ).rejects.toThrow();
  expect(popup.close).toHaveBeenCalledOnce();
});

test("rejects an edge-port token for a different connected edge", async () => {
  vi.stubGlobal(
    "fetch",
    vi.fn().mockResolvedValue(
      Response.json({
        mode: "edge_port",
        edge_api_origin: "https://9.9.9.9:8443",
        token: "secret",
        expires_at: 1000,
      }),
    ),
  );
  await expect(requestPreviewToken(edgeDevice, 5173, signal)).rejects.toThrow();
});
