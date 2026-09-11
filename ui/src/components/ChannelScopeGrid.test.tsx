import { screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { connectedAppState } from "../storybook/mockAppState";
import { renderWithAppProviders } from "../test/render";
import { ChannelScopeGrid } from "./ChannelScopeGrid";

describe("ChannelScopeGrid", () => {
  it("marks All active only when every stored channel pair is scoped", () => {
    const base = connectedAppState.sceneConfigs[0];
    const channelConfigs = [
      { ...base.channelConfigs[0], group: 0, channel: 0 },
      { ...base.channelConfigs[0], group: 0, channel: 1 },
    ];

    renderWithAppProviders(
      <ChannelScopeGrid
        scene={{
          ...base,
          channelConfigs,
          scopedChannels: [
            { group: 0, channel: 0 },
            { group: 99, channel: 99 },
          ],
        }}
      />,
    );

    expect(screen.getByRole("button", { name: "All" })).not.toHaveClass(
      "bg-accent-orange-active",
    );
  });
});
