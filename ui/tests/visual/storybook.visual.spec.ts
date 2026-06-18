import { expect, test } from "@playwright/test";

type StorybookIndex = {
  entries: Record<string, StorybookIndexEntry>;
};

type StorybookIndexEntry = {
  id: string;
  name: string;
  title: string;
  type: "docs" | "story";
};

test("all Storybook stories match visual baselines", async ({ page }) => {
  const response = await page.request.get("/index.json");
  expect(response.ok()).toBe(true);

  const index = (await response.json()) as StorybookIndex;
  const stories = Object.values(index.entries)
    .filter((entry) => entry.type === "story")
    .sort((a, b) => a.id.localeCompare(b.id));

  expect(stories.length).toBeGreaterThan(0);

  for (const story of stories) {
    await test.step(`${story.title}: ${story.name}`, async () => {
      await page.goto(`/iframe.html?id=${story.id}&viewMode=story`);
      await page.locator("#storybook-root").waitFor({ state: "visible" });

      await expect(page).toHaveScreenshot(`${story.id}.png`, {
        fullPage: true,
      });
    });
  }
});
