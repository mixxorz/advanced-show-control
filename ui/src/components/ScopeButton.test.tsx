import { render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { ScopeButton } from "./ScopeButton";
import { ScopeToggleGroup } from "./ScopeToggleGroup";

describe("scope controls", () => {
  it("exposes ScopeButton projected state without becoming a submit control", () => {
    render(
      <ScopeButton active label="1" onClick={vi.fn()} title="Lead vocal" />,
    );

    const button = screen.getByRole("button", { name: "1" });
    expect(button).toHaveAttribute("aria-pressed", "true");
    expect(button).toHaveAttribute("type", "button");
    expect(button).toHaveAttribute("title", "Lead vocal");
  });

  it("exposes each scope-family toggle's independent projected state", () => {
    render(
      <ScopeToggleGroup
        fadersEnabled
        onToggleFaders={vi.fn()}
        onTogglePan={vi.fn()}
        panEnabled={false}
      />,
    );

    expect(screen.getByRole("button", { name: "FADER" })).toHaveAttribute(
      "aria-pressed",
      "true",
    );
    expect(screen.getByRole("button", { name: "PAN" })).toHaveAttribute(
      "aria-pressed",
      "false",
    );
    expect(screen.getByRole("button", { name: "FADER" })).toHaveAttribute(
      "type",
      "button",
    );
  });
});
