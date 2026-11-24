// Run with Bun on the device. Keep each listener exclusive to one loopback family.
for (const [hostname, port] of [["127.0.0.1", 5173], ["::1", 5174]]) {
  const authority = `localhost:${port}`;
  let events;
  Bun.serve({
    hostname, port,
    fetch(request, server) {
      const path = new URL(request.url).pathname;
      const cookies = request.headers.get("cookie") ?? "";
      const previewCookie = cookies.includes("__Host-pontia_preview");
      const appCookie = cookies.split(/;\s*/).includes("app_cookie=present");
      console.log(JSON.stringify({ hostname, path, previewCookie, appCookie }));
      if (request.headers.get("host") !== authority || previewCookie) {
        return new Response("Unexpected host or preview credential", { status: 400 });
      }
      if (path === "/page") {
        return new Response("<!doctype html><title>Loopback preview</title><p id=sec>ready</p>", {
          headers: { "Content-Type": "text/html", "Set-Cookie": "app_cookie=present; Path=/" },
        });
      }
      if (!appCookie) {
        return new Response("Missing application cookie", { status: 400 });
      }
      if (path === "/redirect") return Response.redirect(`http://${authority}/after`, 302);
      if (path === "/after") return new Response(hostname);
      if (path === "/events") {
        return new Response(new ReadableStream({
          start(controller) {
            events = controller;
            controller.enqueue(new TextEncoder().encode("data: ready\n\n"));
          },
        }), { headers: { "Content-Type": "text/event-stream" } });
      }
      if (request.headers.get("origin") !== `http://${authority}`) {
        return new Response("Unexpected origin", { status: 403 });
      }
      if (path === "/ack" && request.method === "POST" && events) {
        events.enqueue(new TextEncoder().encode("data: acknowledged\n\n"));
        events.close();
        events = undefined;
        return new Response(null, { status: 204 });
      }
      if (path === "/hmr" && request.headers.get("sec-websocket-protocol") === "vite-hmr" &&
          server.upgrade(request, { headers: { "Sec-WebSocket-Protocol": "vite-hmr" } })) return;
      return new Response(null, { status: 404 });
    },
    websocket: { message(socket, message) { socket.send(`updated:${message}`); } },
  });
}
