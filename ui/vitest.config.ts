import path from "node:path";
import { fileURLToPath } from "node:url";

import { storybookTest } from "@storybook/addon-vitest/vitest-plugin";
import { playwright } from "@vitest/browser-playwright";
import { defineConfig } from "vitest/config";

const dirname =
  typeof __dirname !== "undefined"
    ? __dirname
    : path.dirname(fileURLToPath(import.meta.url));

export default defineConfig({
  test: {
    passWithNoTests: true,
    projects: [
      {
        extends: "./vite.config.ts",
        test: {
          name: "unit",
          globals: true,
          setupFiles: ["./vitest.setup.ts"],
          environment: "jsdom",
          include: ["src/**/*.{test,spec}.{ts,tsx}"],
          passWithNoTests: true,
        },
      },
      {
        extends: "./vite.config.ts",
        plugins: [
          storybookTest({ configDir: path.join(dirname, ".storybook") }),
        ],
        test: {
          name: "storybook",
          browser: {
            enabled: true,
            headless: true,
            provider: playwright(),
            instances: [{ browser: "chromium" }],
          },
          setupFiles: ["./vitest.setup.ts", "./.storybook/vitest.setup.ts"],
        },
      },
    ],
  },
});
