// On the English homepage, run: playwright-cli run-code --filename tests/browser/wherever-drag.js
// oxlint-disable-next-line no-unused-vars -- playwright-cli invokes this function with its page.
async function verifyWhereverDrag(page) {
  await page.setViewportSize({ width: 1440, height: 1000 });
  await page.emulateMedia({ reducedMotion: "no-preference" });
  const section = page.locator(".wherever");
  const stage = section.locator(".stage");
  const track = section.locator(".slot-track");
  const title = section.locator(".slot");
  await section
    .locator("h2")
    .evaluate((el) => el.scrollIntoView({ block: "center", behavior: "instant" }));
  await stage.locator("path.rope[d]").first().waitFor({ state: "attached" });
  await page.evaluate(() => document.fonts.ready);
  const phone = stage.locator(".you .device");
  await phone.hover();
  const box = await phone.boundingBox();
  const start = { x: box.x + box.width / 2, y: box.y + box.height / 2 };
  await page.mouse.down();
  for (const [index, phrase] of ["at the café", "in the park"].entries()) {
    const ring = await stage.locator(".place-ring").nth(index).boundingBox();
    await page.mouse.move(ring.x + ring.width / 2, ring.y + ring.height / 2);
    await title.evaluate(async (el, phrase) => {
      for (let frame = 0; frame < 10; frame++) {
        if (el.getAttribute("aria-label").includes(phrase)) return;
        await new Promise(requestAnimationFrame);
      }
      throw new Error("Drag missed " + phrase);
    }, phrase);
  }
  await page.mouse.move(start.x, start.y);
  await page.waitForTimeout(120); // Pause before release to stop the phone without a fling.
  await page.mouse.up();
  await page.waitForFunction(() => {
    const words = document.querySelectorAll(".wherever .slot-track .word");
    return words.length === 1 && words[0].textContent === "wherever you are";
  });
  await track.evaluate(async (el) => {
    await Promise.all(
      el
        .getAnimations({ subtree: true })
        .filter((a) => a.effect.getTiming().iterations !== Infinity)
        .map((a) => a.finished),
    );
    const word = el.querySelector(".word");
    const width = el.getBoundingClientRect().width;
    const textWidth = word.getBoundingClientRect().width;
    if (width + 0.5 < textWidth)
      throw new Error(`The ${width}px headline slot clips the ${textWidth}px phrase`);
  });
}
