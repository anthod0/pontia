import type { PublicDeviceTarget } from "./remoteAccess";

let target: PublicDeviceTarget | null = null;
let targetController = new AbortController();

export function setPublicApiTarget(nextTarget: PublicDeviceTarget): void {
  targetController.abort();
  targetController = new AbortController();
  target = Object.freeze({ ...nextTarget });
}

export function clearPublicApiTarget(): void {
  targetController.abort();
  targetController = new AbortController();
  target = null;
}

export function hasPublicApiTarget(): boolean {
  return target !== null;
}

export function publicApiTarget(): PublicDeviceTarget {
  if (!target) throw new Error("Remote device target is not available.");
  return target;
}

export function publicApiSignal(signal?: AbortSignal | null): AbortSignal {
  return signal ? AbortSignal.any([targetController.signal, signal]) : targetController.signal;
}
