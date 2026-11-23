import { render } from "@testing-library/svelte";
import { expect, test } from "vitest";
import ClientIcon from "../../src/components/chat/ClientIcon.svelte";

test("each Antigravity icon references only its own SVG definitions", () => {
  const first = render(ClientIcon, { clientType: "acp", acpAgentId: "antigravity" }).container;
  const second = render(ClientIcon, { clientType: "acp", acpAgentId: "antigravity" }).container;

  const ids = [first, second].map((container) => {
    const svg = container.querySelector("svg")!;
    const references = [...svg.querySelectorAll("[mask], [filter]")].map(
      (element) =>
        (element.getAttribute("mask") ?? element.getAttribute("filter"))!.match(
          /^url\(#(.+)\)$/,
        )![1],
    );
    expect(references.length).toBeGreaterThan(0);
    for (const id of references) expect(svg.querySelector(`[id="${id}"]`)).not.toBeNull();
    return [...svg.querySelectorAll("[id]")].map((element) => element.id);
  });

  expect(ids[0].filter((id) => ids[1].includes(id))).toEqual([]);
});
