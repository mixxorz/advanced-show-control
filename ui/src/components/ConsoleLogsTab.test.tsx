import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { MockAppProviders } from "../storybook/MockAppProviders";
import {
  connectedAppState,
  discoveringAppState,
} from "../storybook/mockAppState";
import { ConsoleLogsTab } from "./ConsoleLogsTab";

describe("ConsoleLogsTab", () => {
  it("renders frontend logs with severity labels", () => {
    render(
      <MockAppProviders appState={connectedAppState}>
        <ConsoleLogsTab />
      </MockAppProviders>,
    );

    expect(screen.getByRole("heading", { name: "Logs" })).toBeInTheDocument();
    expect(
      screen.getByText("Connected to LV1 at 192.168.1.42:22000"),
    ).toBeInTheDocument();
    expect(screen.getByText("WARNING")).toBeInTheDocument();
  });

  it("shows the empty state when there are no logs", () => {
    render(
      <MockAppProviders appState={discoveringAppState}>
        <ConsoleLogsTab />
      </MockAppProviders>,
    );

    expect(screen.getByText("No frontend logs yet.")).toBeInTheDocument();
  });
});
