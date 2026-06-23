import { render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { connectedAppState } from "../storybook/mockAppState";
import { SceneListRow } from "./SceneListRow";

describe("SceneListRow", () => {
  it("renders unlinked scene rows with warning styling", () => {
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

    expect(screen.getByText("--")).toBeInTheDocument();
    expect(screen.getByLabelText("Unlinked scene")).toBeInTheDocument();
  });
});
