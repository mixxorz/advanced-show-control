import { expect, test } from "@playwright/test";

const stories = [
  ["AppShell", "app-appshell--scene-tab"],
  ["ConnectionScreen", "components-connectionscreen--systems-found"],
  ["Header", "components-header--connected"],
  ["LogsTab", "components-logstab--populated"],
  ["SceneTab", "components-scenetab--stored-scene-selected"],
  ["StatusBadge", "components-statusbadge--good"],
] as const;

for (const [componentName, storyId] of stories) {
  test(`${componentName} story matches visual baseline`, async ({ page }) => {
    await page.goto(`/iframe.html?id=${storyId}&viewMode=story`);
    await page.locator("#storybook-root").waitFor({ state: "visible" });

    await expect(page).toHaveScreenshot(`${storyId}.png`, {
      fullPage: true,
    });
  });
}
