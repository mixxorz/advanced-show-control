import { storybookTest } from "@storybook/addon-vitest/vitest-plugin";
import { playwright } from "@vitest/browser-playwright";
import { defineConfig } from "vitest/config";

export default defineConfig({
  test: {
    passWithNoTests: true,
    projects: [
      {
        extends: "./vite.config.ts",
        test: {
          name: "unit",
          include: ["src/**/*.{test,spec}.{ts,tsx}"],
        },
      },
      {
        extends: "./vite.config.ts",
        plugins: [storybookTest({ configDir: ".storybook" })],
        test: {
          name: "storybook",
          browser: {
            enabled: true,
            headless: true,
            provider: playwright(),
            instances: [{ browser: "chromium" }],
          },
          setupFiles: ["./vitest.setup.ts"],
        },
      },
    ],
  },
});
