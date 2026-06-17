import { defineConfig } from "vitest/config";

import workspace from "./vitest.workspace";

export default defineConfig({
  test: {
    passWithNoTests: true,
    projects: workspace,
  },
});
