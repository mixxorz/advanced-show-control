import { render, type RenderOptions } from "@testing-library/react";
import type { ReactElement } from "react";
import type { AppCommands } from "../appContext";
import { MockAppProviders } from "../storybook/MockAppProviders";
import type { AppViewState } from "../types";

export function renderWithAppProviders(
  ui: ReactElement,
  options: RenderOptions & {
    appState?: AppViewState;
    commandError?: string | null;
    commands?: Partial<AppCommands>;
  } = {},
) {
  const { appState, commandError, commands, ...renderOptions } = options;

  return render(ui, {
    wrapper: ({ children }) => (
      <MockAppProviders
        appState={appState}
        commandError={commandError}
        commands={commands}
      >
        {children}
      </MockAppProviders>
    ),
    ...renderOptions,
  });
}
