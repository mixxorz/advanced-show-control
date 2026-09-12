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

  it("keeps the left border transparent when a current row is not selected", () => {
    const scene = connectedAppState.sceneConfigs[0];
    render(
      <SceneListRow
        currentScene={{ index: scene.sceneIndex!, name: scene.sceneName }}
        cued
        onSelect={vi.fn()}
        scene={scene}
        selected={false}
      />,
    );

    expect(screen.getByRole("button")).toHaveClass("border-l-transparent");
    expect(screen.getByRole("button")).not.toHaveClass(
      "border-l-status-current",
    );
  });

  it("uses current then cued precedence for selected-row border chrome", () => {
    const scene = connectedAppState.sceneConfigs[0];
    const { rerender } = render(
      <SceneListRow
        currentScene={{ index: scene.sceneIndex!, name: scene.sceneName }}
        cued
        onSelect={vi.fn()}
        scene={scene}
        selected
      />,
    );

    expect(screen.getByRole("button")).toHaveClass("border-l-status-current");

    rerender(
      <SceneListRow
        currentScene={null}
        cued
        onSelect={vi.fn()}
        scene={scene}
        selected
      />,
    );

    expect(screen.getByRole("button")).toHaveClass("border-l-status-cued");
  });
});
