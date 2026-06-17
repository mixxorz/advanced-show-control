import "@testing-library/jest-dom/vitest";
import { render, screen } from "@testing-library/react";
import { act } from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { MockAppProviders } from "../storybook/MockAppProviders";
import { connectedAppState } from "../storybook/mockAppState";
import { BottomStatusBar } from "./BottomStatusBar";

describe("BottomStatusBar", () => {
  beforeEach(() => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date("2026-06-17T20:14:02Z"));

    vi.spyOn(Intl, "DateTimeFormat").mockImplementation(
      function DateTimeFormatMock() {
        return {
          format: (date: Date) => date.toISOString().slice(11, 19),
        } as Intl.DateTimeFormat;
      } as typeof Intl.DateTimeFormat,
    );
  });

  afterEach(() => {
    vi.restoreAllMocks();
    vi.useRealTimers();
  });

  it("renders the current, selected, and connection state", () => {
    render(
      <MockAppProviders appState={connectedAppState}>
        <BottomStatusBar appState={connectedAppState} />
      </MockAppProviders>,
    );

    expect(screen.getByText("Mode")).toBeInTheDocument();
    expect(screen.getByText("Current")).toBeInTheDocument();
    expect(screen.getByText("Selected")).toBeInTheDocument();
    expect(screen.getByText("Connection")).toBeInTheDocument();
    expect(screen.getByText("Sync")).toBeInTheDocument();
    expect(screen.getByText("Time")).toBeInTheDocument();

    expect(screen.getAllByText("4 Verse")).toHaveLength(2);
    expect(screen.getByText("Connected to FOH LV1")).toBeInTheDocument();
    expect(screen.getByText("In Sync")).toBeInTheDocument();
    expect(screen.getByText("20:14:02")).toBeInTheDocument();
  });

  it("updates the clock every second", () => {
    render(
      <MockAppProviders appState={connectedAppState}>
        <BottomStatusBar appState={connectedAppState} />
      </MockAppProviders>,
    );

    expect(screen.getByText("20:14:02")).toBeInTheDocument();

    act(() => {
      vi.advanceTimersByTime(1000);
    });

    expect(screen.getByText("20:14:03")).toBeInTheDocument();
  });
});
