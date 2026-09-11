import { render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { connectedAppState } from "../storybook/mockAppState";
import { SceneListRow } from "./SceneListRow";

describe("SceneListRow", () => {
  it("shows a placeholder number for an unlinked scene", () => {
    render(
      <SceneListRow
        currentScene={null}
        cued={false}
        onSelect={vi.fn()}
        scene={{
          ...connectedAppState.sceneConfigs[0],
          sceneIndex: null,
        }}
        selected={false}
      />,
    );

    expect(screen.getByText("---")).toBeInTheDocument();
  });
});
