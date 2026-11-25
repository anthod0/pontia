import { render, screen, fireEvent } from "@testing-library/svelte";
import { expect, test, vi } from "vitest";
import SecretInputDialog from "../../src/components/chat/SecretInputDialog.svelte";

test("keeps pasted CRLF bytes through subsequent edits and pasted ranges", async () => {
  const onConfirm = vi.fn();
  render(SecretInputDialog, { open: true, onConfirm, onClose: vi.fn() });
  const paste = (target: HTMLElement, text: string) =>
    fireEvent.paste(target, { clipboardData: { getData: () => text } });
  await paste(screen.getByLabelText("Secret", { exact: true }), "a\r\nb");
  const field = screen.getByLabelText("Secret", { exact: true }) as HTMLTextAreaElement;
  field.setSelectionRange(field.value.length, field.value.length);
  await paste(field, "x\ny");
  field.value += "!";
  await fireEvent.input(field);
  await fireEvent.click(screen.getByRole("button", { name: "Add", exact: true }));
  expect(onConfirm).toHaveBeenCalledWith({ value: "a\r\nbx\ny!", label: undefined });
});

test("keeps the optional label independent of the password and clears both on cancel", async () => {
  const onConfirm = vi.fn();
  const view = render(SecretInputDialog, { open: true, onConfirm, onClose: vi.fn() });
  const secret = screen.getByLabelText("Secret", { exact: true });
  const label = screen.getByLabelText("Label (optional)");
  await fireEvent.input(secret, { target: { value: "sample-credential" } });
  expect(label).toHaveValue("");
  await fireEvent.input(label, { target: { value: " Dev " } });
  expect(secret).toHaveValue("sample-credential");
  await fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
  expect(onConfirm).not.toHaveBeenCalled();
  await view.rerender({ open: true, onConfirm, onClose: vi.fn() });
  expect(screen.getByLabelText("Secret", { exact: true })).toHaveValue("");
  expect(screen.getByLabelText("Label (optional)")).toHaveValue("");
  await fireEvent.input(screen.getByLabelText("Secret", { exact: true }), {
    target: { value: "new-sample" },
  });
  await fireEvent.input(screen.getByLabelText("Label (optional)"), { target: { value: " Dev " } });
  await fireEvent.click(screen.getByRole("button", { name: "Add", exact: true }));
  expect(onConfirm).toHaveBeenCalledWith({ value: "new-sample", label: "Dev" });
});
