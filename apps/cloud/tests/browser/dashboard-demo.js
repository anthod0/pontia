// Run on the English homepage with playwright-cli run-code --filename tests/browser/dashboard-demo.js
// oxlint-disable-next-line no-unused-vars -- playwright-cli supplies the page.
async function verifyDashboardDemo(page) {
  const demo = page.locator(".dashboard-demo");
  const tab = (name) => demo.getByRole("tab", { name, exact: true });
  const row = (key) => demo.locator(`[data-git-row="${key}"]`);
  const escapeTo = async (key) => {
    await page.keyboard.press("Escape");
    await row(key).waitFor();
    if (!(await row(key).evaluate((el) => el === document.activeElement))) {
      throw new Error("Expected keyboard focus to return to " + key);
    }
  };
  await page.setViewportSize({ width: 1440, height: 900 });
  const notes = demo.getByRole("button", { name: "Preview api-review.md" }).first();
  await notes.click();
  await tab("Source").click();
  await demo.getByRole("button", { name: "Close file" }).click();
  await demo
    .getByRole("region", { name: "Recently opened" })
    .getByRole("button", { name: "Preview api-review.md" })
    .click();
  await demo.locator(".demo-md h1").waitFor();
  await tab("Git").click();
  await tab("History").click();
  await row("4c2a9d8").click();
  await row("src/lib/CommandMenu.svelte").click();
  await escapeTo("src/lib/CommandMenu.svelte");
  await escapeTo("4c2a9d8");
  await page.keyboard.press("Escape");
  await tab("Git").waitFor({ state: "hidden" });

  await demo.getByRole("button", { name: /Add keyboard shortcuts/ }).click();
  await demo.getByRole("link", { name: /localhost:5173/ }).click();
  const address = demo.getByRole("textbox", { name: "Preview address" });
  await address.fill("https://example.org/settings");
  await address.press("Enter");
  if ((await address.getAttribute("aria-invalid")) !== "true")
    throw new Error("Accepted a different origin");
  await address.fill("/settings");
  await address.press("Enter");
  await demo.locator(".atlas .panel").first().waitFor();
  await address.press("Control+k");
  const menu = demo.getByRole("dialog", { name: "Command menu" });
  await menu.waitFor();
  await page.keyboard.press("Escape");
  await menu.waitFor({ state: "hidden" });
  await tab("Files").click();
  await tab("Preview").click();
  if ((await address.inputValue()) !== "http://localhost:5173/settings")
    throw new Error("Lost preview address");

  await page.setViewportSize({ width: 390, height: 844 });
  await demo.getByRole("button", { name: "Back", exact: true }).click();
  await demo.getByRole("link", { name: /localhost:5173/ }).click();
  const browserTab = demo.getByRole("region", { name: /Browser tab:/ });
  await browserTab.waitFor();
  await page.keyboard.press("Control+k");
  await menu.waitFor();
  await page.keyboard.press("Escape");
  await menu.waitFor({ state: "hidden" });
  await browserTab.getByRole("button", { name: "Close tab and return to the Dashboard" }).click();
  await browserTab.waitFor({ state: "hidden" });
  await demo.getByRole("link", { name: /localhost:5173/ }).waitFor();
}
