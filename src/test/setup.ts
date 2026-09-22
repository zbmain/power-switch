import "@testing-library/jest-dom/vitest";
import { cleanup } from "@testing-library/react";
import { afterEach, vi } from "vitest";

afterEach(cleanup);
Object.defineProperty(window, "matchMedia", {
  writable: true,
  value: () => ({
    matches: false,
    addEventListener: vi.fn(),
    removeEventListener: vi.fn(),
  }),
});
globalThis.ResizeObserver = class ResizeObserver {
  /** Test DOMs have no layout changes to observe. */ observe() {}
  /** Release the no-op observation. */ unobserve() {}
  /** Release every no-op observation. */ disconnect() {}
};
