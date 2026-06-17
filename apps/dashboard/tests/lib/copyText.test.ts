import { afterEach, describe, expect, test, vi } from 'vitest';
import { copyText } from '../../src/lib/copyText';

const originalClipboard = navigator.clipboard;
const originalExecCommand = document.execCommand;

afterEach(() => {
  vi.restoreAllMocks();
  Object.defineProperty(navigator, 'clipboard', { configurable: true, value: originalClipboard });
  Object.defineProperty(document, 'execCommand', { configurable: true, value: originalExecCommand });
  document.body.innerHTML = '';
});

describe('copyText', () => {
  test('uses navigator clipboard when available', async () => {
    const writeText = vi.fn().mockResolvedValue(undefined);
    Object.defineProperty(navigator, 'clipboard', { configurable: true, value: { writeText } });
    document.execCommand = vi.fn().mockReturnValue(true);
    const execCommand = vi.mocked(document.execCommand);

    await expect(copyText('assistant reply')).resolves.toBe(true);

    expect(writeText).toHaveBeenCalledWith('assistant reply');
    expect(execCommand).not.toHaveBeenCalled();
  });

  test('falls back to execCommand copy when navigator clipboard is unavailable', async () => {
    Object.defineProperty(navigator, 'clipboard', { configurable: true, value: undefined });
    document.execCommand = vi.fn().mockReturnValue(true);
    const execCommand = vi.mocked(document.execCommand);

    await expect(copyText('assistant reply over http')).resolves.toBe(true);

    expect(execCommand).toHaveBeenCalledWith('copy');
    expect(document.body.querySelector('textarea')).not.toBeInTheDocument();
  });

  test('falls back to execCommand copy when navigator clipboard rejects', async () => {
    const writeText = vi.fn().mockRejectedValue(new Error('secure context required'));
    Object.defineProperty(navigator, 'clipboard', { configurable: true, value: { writeText } });
    document.execCommand = vi.fn().mockReturnValue(true);
    const execCommand = vi.mocked(document.execCommand);

    await expect(copyText('assistant reply on insecure origin')).resolves.toBe(true);

    expect(writeText).toHaveBeenCalledWith('assistant reply on insecure origin');
    expect(execCommand).toHaveBeenCalledWith('copy');
  });
});
