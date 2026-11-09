import { defineConfig } from "vitest/config";

// `client/*.test.ts`: prove with the generated client, then send the proof to the
// program in LiteSVM. `just build` makes what they read.
export default defineConfig({
  test: {
    include: ["client/*.test.ts"],
    globals: true,
    testTimeout: 300_000,
    hookTimeout: 300_000,
  },
});
