// Start the browser signed in with this run's seeded Cloud session.
import { readFile } from "node:fs/promises";

const cookie = JSON.parse(await readFile("/run/secrets/browser-login.json", "utf8"));
// Wait until Chromium has opened its startup tab.
let page;
for (let attempt = 0; !page; attempt++) {
  if (attempt >= 600) throw new Error("Chromium opened no page");
  if (attempt) await new Promise((resolve) => setTimeout(resolve, 100));
  const targets = await fetch("http://127.0.0.1:9222/json/list").then((r) => r.json(), () => []);
  page = targets.find((target) => target.type === "page");
}
const { webSocketDebuggerUrl: endpoint } = await (
  await fetch("http://127.0.0.1:9222/json/version")
).json();
const socket = new WebSocket(endpoint);
await new Promise((resolve, reject) => {
  socket.addEventListener("open", resolve, { once: true });
  socket.addEventListener("error", reject, { once: true });
});
const pending = new Map();
let nextId = 0;
socket.addEventListener("message", (event) => {
  const message = JSON.parse(event.data);
  const request = pending.get(message.id);
  if (!request) return;
  pending.delete(message.id);
  if (message.error) request.reject(new Error(message.error.message));
  else request.resolve(message.result);
});
function send(method, params = {}, sessionId) {
  const id = ++nextId;
  socket.send(JSON.stringify({ id, method, params, sessionId }));
  return new Promise((resolve, reject) => pending.set(id, { resolve, reject }));
}

await send("Storage.setCookies", { cookies: [cookie] });
const { sessionId } = await send("Target.attachToTarget", { targetId: page.id, flatten: true });
await send("Page.navigate", { url: "https://pontia.dev/settings/account" }, sessionId);
socket.close();
