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
  await page.clock.setFixedTime(new Date("2026-01-01T12:00:00Z"));

  const response = await page.request.get("/index.json");
  expect(response.ok()).toBe(true);

  const index = (await response.json()) as StorybookIndex;
  const stories = Object.values(index.entries)
    .filter((entry) => entry.type === "story")
    .sort((a, b) => a.id.localeCompare(b.id));

  expect(stories.length).toBeGreaterThan(0);

  const failures: string[] = [];

  for (const story of stories) {
    await test.step(`${story.title}: ${story.name}`, async () => {
      await page.goto(`/iframe.html?id=${story.id}&viewMode=story`);
      await page.locator("#storybook-root").waitFor({ state: "visible" });

      try {
        await expect(page).toHaveScreenshot(`${story.id}.png`, {
          fullPage: true,
        });
      } catch (error) {
        failures.push(
          `${story.title}: ${story.name}\n${error instanceof Error ? error.message : String(error)}`,
        );
      }
    });
  }

  expect(failures, failures.join("\n\n")).toHaveLength(0);
});
