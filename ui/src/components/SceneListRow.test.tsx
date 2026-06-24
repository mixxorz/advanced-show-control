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

    expect(screen.getByText("---")).toBeInTheDocument();
    expect(screen.queryByLabelText("Unlinked scene")).not.toBeInTheDocument();
  });

  it.each([
    {
      name: "idle",
      borderClass: "border-l-accent-orange",
      arrowClass: "text-accent-orange",
    },
    {
      name: "current",
      current: true,
      borderClass: "border-l-status-current",
      arrowClass: "text-status-current",
    },
    {
      name: "unlinked",
      unlinked: true,
      borderClass: "border-l-status-warning",
      arrowClass: "text-status-warning",
    },
  ])(
    "uses matching arrow and left stripe colors for selected $name rows",
    ({ arrowClass, borderClass, current, unlinked }) => {
      const scene = {
        ...connectedAppState.sceneConfigs[0],
        sceneIndex: unlinked
          ? null
          : connectedAppState.sceneConfigs[0].sceneIndex,
      };

      const { container } = render(
        <SceneListRow
          currentScene={
            current
              ? { index: scene.sceneIndex ?? 0, name: scene.sceneName }
              : null
          }
          cued={false}
          onSelect={vi.fn()}
          scene={scene}
          selected={true}
        />,
      );

      const row = screen.getByRole("button");
      const arrow = container.querySelector("svg");

      expect(row).toHaveClass("bg-accent-orange-soft");
      expect(row).toHaveClass(borderClass);
      expect(arrow).toHaveClass(arrowClass);
    },
  );
});
