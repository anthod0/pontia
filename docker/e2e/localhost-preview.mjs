import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { chromium } from "/opt/browser/node_modules/playwright-core/index.mjs";

const [dashboardUrl] = process.argv.slice(2);
const { device, session, marker } = JSON.parse(readFileSync(0, "utf8"));
const browser = await chromium.connectOverCDP("http://127.0.0.1:9222");
const [context] = browser.contexts();
context.setDefaultTimeout(30_000);
const dashboard = await context.newPage();
const requests = [];
context.on("request", request => requests.push(request.url()));
await dashboard.goto(`${dashboardUrl}/chat/${session}`);
const resultFile = dashboard.locator(`button[title$="preview-${marker}.txt"]`);
await resultFile.click({ timeout: 90_000 });
await dashboard.locator("[data-file-preview-content]").getByText(marker, { exact: true }).waitFor();

async function openPreview(port, status) {
  const path = "/page?a=1#sec";
  const origin = `https://${port}-${device.replaceAll("-", "")}.docker-e2e.p.pontia.dev`;
  const [response, popup, exchange] = await Promise.all([
    context.waitForEvent("response", r => r.url().split("#")[0] === origin + "/page?a=1"),
    context.waitForEvent("page"),
    context.waitForEvent("response", r => r.url() === origin + "/__pontia_auth"),
    dashboard.locator(`a[href="http://localhost:${port}${path}"]`).last().click(),
  ]);
  assert.equal(exchange.status(), 303);
  assert.equal(exchange.request().method(), "POST");
  assert.equal(response.status(), status);
  await popup.waitForURL(origin + path);
  const cdp = await context.newCDPSession(popup);
  const history = await cdp.send("Page.getNavigationHistory");
  await cdp.detach();
  const token = new URLSearchParams(exchange.request().postData()).get("token");
  assert.ok(token && [...requests, ...history.entries.map(entry => entry.url)]
    .every(url => !url.includes(token)), "A URL contained the preview credential");
  return popup;
}

async function checkStreams(page) {
  return page.evaluate(async () => {
    const ws = new WebSocket(`wss://${location.host}/hmr`, "vite-hmr");
    const events = new EventSource("/events");
    try {
      const exchange = new Promise((resolve, reject) => {
        ws.onerror = () => reject(Error("WebSocket handshake failed"));
        ws.onopen = () => ws.send("change");
        ws.onmessage = event => resolve([ws.protocol, event.data]);
      });
      const nextEvent = () => new Promise((resolve, reject) => {
        events.onerror = () => reject(Error("EventSource failed"));
        events.onmessage = event => resolve(event.data);
      });
      const stream = (async () => {
        if (await nextEvent() !== "ready") throw Error("Missing first SSE event");
        const acknowledged = nextEvent();
        const [response, message] = await Promise.all([fetch("/ack", { method: "POST" }), acknowledged]);
        if (response.status !== 204) throw Error("SSE ack failed");
        return message;
      })();
      return await Promise.race([
        Promise.all([exchange, stream]),
        new Promise((_, reject) => setTimeout(() => reject(Error("Stream timed out")), 10_000)),
      ]);
    } finally {
      ws.close();
      events.close();
    }
  });
}

async function checkErrorPage(page) {
  await page.getByRole("heading").waitFor();
  await page.locator('a[href="https://app.pontia.dev"]').waitFor();
}

for (const [port, family] of [[5173, "127.0.0.1"], [5174, "::1"]]) {
  const page = await openPreview(port, 200);
  assert.ok(await page.evaluate(() => document.cookie === "app_cookie=present"), "Unexpected script-visible cookies");
  const cookie = (await context.cookies(page.url())).find(c => c.name === "__Host-pontia_preview");
  assert.ok(cookie?.httpOnly && cookie.secure && cookie.domain === new URL(page.url()).hostname);
  const remaining = cookie.expires - Date.now() / 1000;
  assert.ok(remaining > 4 * 3600 - 60 && remaining <= 4 * 3600);
  assert.deepEqual(await checkStreams(page), [["vite-hmr", "updated:change"], "acknowledged"]);
  const origin = new URL(page.url()).origin;
  const after = await page.goto(origin + "/redirect");
  assert.equal(after.request().redirectedFrom()?.url(), origin + "/redirect");
  assert.equal(page.url(), origin + "/after");
  assert.equal(await after.text(), family);
  await context.clearCookies({ name: cookie.name, domain: cookie.domain });
  assert.equal((await page.reload()).status(), 401);
  await checkErrorPage(page);
  await page.close();
  console.log(`PASS ${family} HTTP, credentials, redirect, SSE, vite-hmr and 401`);
}
await checkErrorPage(await openPreview(5175, 502));
console.log(`PASS closed port 502 and Dashboard Session/file over the control tunnel: ${session}`);
await browser.close();
