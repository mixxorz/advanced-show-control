import { useState, type ComponentProps } from "react";
import type { Meta, StoryObj } from "@storybook/react-vite";
import { expect, within } from "storybook/test";
import {
  AppCommandsProvider,
  AppStateProvider,
  type AppCommands,
} from "../appContext";
import { KeyboardProvider } from "../keyboard";
import { disconnectedAppViewState } from "../types";
import {
  connectedAppState,
  discoveredSystemsAppState,
  discoveringAppState,
} from "../storybook/mockAppState";
import { mockAppCommands } from "../storybook/mockAppCommands";
import type { AppSettings, AppViewState, SceneConfig } from "../types";
import { AppShell } from "./AppShell";

type AppShellStoryArgs = ComponentProps<typeof AppShell> & {
  appState?: AppViewState;
  commandError?: string | null;
};

const shellSceneNames = [
  "Service Start",
  "Tuning: A",
  "S01: The Wonderful Blood",
  "S01: The Wonderful Blood - Down",
  "S02: Holy Forever",
  "S02: Holy Forever - Big",
  "S05: Hark The Herald Angels Sing",
  "S05: Hark - Down",
  "Message Intro",
  "Message",
  "Response",
  "Service Close",
  "Walk Out",
];

function makeShellSceneConfig(index: number, name: string): SceneConfig {
  const source = connectedAppState.sceneConfigs[index % 2];

  return {
    ...source,
    internalSceneId: `app-shell-scene-${index}`,
    sceneIndex: index,
    sceneName: name,
    durationMs: index === 0 ? 0 : (index % 6) * 500 + 1000,
  };
}

const shellSceneConfigs = shellSceneNames.map((name, index) =>
  makeShellSceneConfig(index, name),
);

const sceneTabAppState: AppViewState = {
  ...connectedAppState,
  cuedSceneInternalId: shellSceneConfigs[5].internalSceneId,
  currentScene: { index: 2, name: "S01: The Wonderful Blood" },
  sceneConfigs: shellSceneConfigs,
  selectedSceneInternalId: shellSceneConfigs[6].internalSceneId,
};

const offlineSceneTabAppState: AppViewState = {
  ...sceneTabAppState,
  connection: "disconnected",
  connectedLv1Identity: null,
  currentScene: null,
  discoveredLv1Systems: discoveredSystemsAppState.discoveredLv1Systems,
  cuedSceneInternalId: null,
};

const meta: Meta<AppShellStoryArgs> = {
  title: "App/AppShell",
  component: AppShell,
  parameters: {
    layout: "fullscreen",
  },
  args: {
    activeTab: "scenes",
    onOpenConnection: () => {},
    onResume: () => {},
    onSelectTab: () => {},
    showConnection: false,
  },
  render: (args) => (
    <KeyboardProvider>
      <StatefulAppShellStory
        appShellProps={args}
        commandError={args.commandError}
        initialAppState={args.appState}
      />
    </KeyboardProvider>
  ),
};

function StatefulAppShellStory(props: {
  appShellProps: ComponentProps<typeof AppShell>;
  commandError?: string | null;
  initialAppState?: AppViewState;
}) {
  const [appState, setAppState] = useState(
    props.initialAppState ?? disconnectedAppViewState,
  );

  const commands: AppCommands = {
    ...mockAppCommands,
    cueScene: (internalSceneId) =>
      setAppState((state) => ({
        ...state,
        cuedSceneInternalId: internalSceneId,
      })),
    recallScene: (internalSceneId) =>
      setAppState((state) => {
        const scene = state.sceneConfigs.find(
          (entry) => entry.internalSceneId === internalSceneId,
        );
        if (!scene || scene.sceneIndex == null) return state;

        return {
          ...state,
          currentScene: { index: scene.sceneIndex ?? 0, name: scene.sceneName },
        };
      }),
    selectScene: (internalSceneId) =>
      setAppState((state) => ({
        ...state,
        selectedSceneInternalId: internalSceneId,
      })),
    setAllChannelsScoped: (_internalSceneId, scoped) =>
      setAppState((state) => {
        const selectedSceneInternalId = state.selectedSceneInternalId;
        if (!selectedSceneInternalId) return state;

        return {
          ...state,
          sceneConfigs: state.sceneConfigs.map((scene) =>
            scene.internalSceneId === selectedSceneInternalId
              ? {
                  ...scene,
                  scopedChannels: scoped
                    ? scene.channelConfigs.map((config) => ({
                        group: config.group,
                        channel: config.channel,
                      }))
                    : [],
                }
              : scene,
          ),
        };
      }),
    setChannelScoped: (_internalSceneId, group, channel, scoped) =>
      setAppState((state) => {
        const selectedSceneInternalId = state.selectedSceneInternalId;
        if (!selectedSceneInternalId) return state;

        return {
          ...state,
          sceneConfigs: state.sceneConfigs.map((scene) => {
            if (scene.internalSceneId !== selectedSceneInternalId) return scene;

            const nextScopedChannels = scoped
              ? [
                  ...scene.scopedChannels.filter(
                    (entry) =>
                      entry.group !== group || entry.channel !== channel,
                  ),
                  { group, channel },
                ]
              : scene.scopedChannels.filter(
                  (entry) => entry.group !== group || entry.channel !== channel,
                );

            return { ...scene, scopedChannels: nextScopedChannels };
          }),
        };
      }),
    setSceneDurationMs: async (_internalSceneId, durationMs) => {
      setAppState((state) => {
        const selectedSceneInternalId = state.selectedSceneInternalId;
        if (!selectedSceneInternalId) return state;

        return {
          ...state,
          sceneConfigs: state.sceneConfigs.map((scene) =>
            scene.internalSceneId === selectedSceneInternalId
              ? { ...scene, durationMs }
              : scene,
          ),
        };
      });
      return true;
    },
    setSceneScopeFadersEnabled: (_internalSceneId, enabled) =>
      setAppState((state) =>
        updateSelectedSceneToggle(state, "faders", enabled),
      ),
    setSceneScopePanEnabled: (_internalSceneId, enabled) =>
      setAppState((state) => updateSelectedSceneToggle(state, "pan", enabled)),
    toggleLockout: () =>
      setAppState((state) => ({ ...state, lockout: !state.lockout })),
  };

  function replaceSettings(settings: AppSettings) {
    setAppState((state) => ({ ...state, settings }));
  }

  return (
    <AppStateProvider
      appState={appState}
      commandError={props.commandError ?? null}
    >
      <AppCommandsProvider commands={commands}>
        <AppShell
          activeTab={props.appShellProps.activeTab}
          onOpenConnection={props.appShellProps.onOpenConnection}
          onReplaceSettings={replaceSettings}
          onResume={props.appShellProps.onResume}
          onSelectTab={props.appShellProps.onSelectTab}
          showConnection={props.appShellProps.showConnection}
        />
      </AppCommandsProvider>
    </AppStateProvider>
  );
}

function updateSelectedSceneToggle(
  state: AppViewState,
  toggle: "faders" | "pan",
  enabled: boolean,
): AppViewState {
  const selectedSceneInternalId = state.selectedSceneInternalId;
  if (!selectedSceneInternalId) return state;

  return {
    ...state,
    sceneConfigs: state.sceneConfigs.map((scene) =>
      scene.internalSceneId === selectedSceneInternalId
        ? {
            ...scene,
            scopeToggles: { ...scene.scopeToggles, [toggle]: enabled },
          }
        : scene,
    ),
  };
}

export default meta;

type Story = StoryObj<AppShellStoryArgs>;

export const ConnectionSearching: Story = {
  args: {
    appState: discoveringAppState,
    showConnection: true,
  },
};

export const ConnectionSystemsFound: Story = {
  args: {
    appState: offlineSceneTabAppState,
    showConnection: true,
  },
};

export const SceneTab: Story = {
  args: {
    appState: sceneTabAppState,
  },
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);

    await expect(
      canvas.getByRole("heading", { name: "Scene List" }),
    ).toBeInTheDocument();
    await expect(
      canvas.getByRole("button", { name: "Scenes" }),
    ).toBeInTheDocument();
    await expect(
      canvas.getByRole("button", { name: "Settings" }),
    ).toBeInTheDocument();
  },
};

export const LogsTab: Story = {
  args: {
    activeTab: "logs",
  },
};

export const SettingsTab: Story = {
  args: {
    activeTab: "settings",
    appState: connectedAppState,
  },
};

export const CommandError: Story = {
  args: {
    commandError: "Unable to save session: permission denied.",
  },
};

export const ReconnectOverlay: Story = {
  args: {
    appState: {
      ...connectedAppState,
      reconnect: { active: true, attempt: 2 },
    },
  },
};

export const EmptyMainShell: Story = {
  args: {
    appState: disconnectedAppViewState,
  },
};
