export { cn } from "cn";

export type WithElementRef<T, U extends HTMLElement = HTMLElement> = T & {
  ref?: U | null;
};

export type WithoutChildrenOrChild<T> = Omit<T, "children" | "child">;

export type WithoutChild<T> = T extends { child?: unknown } ? Omit<T, "child"> : T;
