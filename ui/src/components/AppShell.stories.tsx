import { useState, type ComponentProps, type ReactNode } from "react";
import type { Meta, StoryObj } from "@storybook/react-vite";
import { expect, within } from "storybook/test";
import {
  AppCommandsProvider,
  AppStateProvider,
  type AppCommands,
} from "../appContext";
import { disconnectedAppViewState } from "../types";
import {
  connectedAppState,
  discoveredSystemsAppState,
  discoveringAppState,
} from "../storybook/mockAppState";
import { mockAppCommands } from "../storybook/mockAppCommands";
import type { AppViewState, SceneConfig } from "../types";
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
    sceneId: `app-shell-scene-${index}`,
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
  cuedSceneId: shellSceneConfigs[5].sceneId,
  currentScene: { index: 2, name: "S01: The Wonderful Blood" },
  sceneConfigs: shellSceneConfigs,
  selectedSceneId: shellSceneConfigs[6].sceneId,
};

const offlineSceneTabAppState: AppViewState = {
  ...sceneTabAppState,
  connection: "disconnected",
  connectedLv1Identity: null,
  currentScene: null,
  discoveredLv1Systems: discoveredSystemsAppState.discoveredLv1Systems,
  cuedSceneId: null,
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
    <StatefulAppShellStory
      commandError={args.commandError}
      initialAppState={args.appState}
    >
      <AppShell
        activeTab={args.activeTab}
        onOpenConnection={args.onOpenConnection}
        onResume={args.onResume}
        onSelectTab={args.onSelectTab}
        showConnection={args.showConnection}
      />
    </StatefulAppShellStory>
  ),
};

function StatefulAppShellStory(props: {
  children: ReactNode;
  commandError?: string | null;
  initialAppState?: AppViewState;
}) {
  const [appState, setAppState] = useState(
    props.initialAppState ?? disconnectedAppViewState,
  );

  const commands: AppCommands = {
    ...mockAppCommands,
    cueScene: (sceneId) =>
      setAppState((state) => ({ ...state, cuedSceneId: sceneId })),
    recallScene: (sceneId) =>
      setAppState((state) => {
        const scene = state.sceneConfigs.find(
          (entry) => entry.sceneId === sceneId,
        );
        if (!scene) return state;

        return {
          ...state,
          currentScene: { index: scene.sceneIndex, name: scene.sceneName },
        };
      }),
    selectScene: (sceneId) =>
      setAppState((state) => ({ ...state, selectedSceneId: sceneId })),
    setAllChannelsScoped: (_sceneId, scoped) =>
      setAppState((state) => {
        const selectedSceneId = state.selectedSceneId;
        if (!selectedSceneId) return state;

        return {
          ...state,
          sceneConfigs: state.sceneConfigs.map((scene) =>
            scene.sceneId === selectedSceneId
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
    setChannelScoped: (_sceneId, group, channel, scoped) =>
      setAppState((state) => {
        const selectedSceneId = state.selectedSceneId;
        if (!selectedSceneId) return state;

        return {
          ...state,
          sceneConfigs: state.sceneConfigs.map((scene) => {
            if (scene.sceneId !== selectedSceneId) return scene;

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
    setSceneDurationMs: async (_sceneId, durationMs) => {
      setAppState((state) => {
        const selectedSceneId = state.selectedSceneId;
        if (!selectedSceneId) return state;

        return {
          ...state,
          sceneConfigs: state.sceneConfigs.map((scene) =>
            scene.sceneId === selectedSceneId
              ? { ...scene, durationMs }
              : scene,
          ),
        };
      });
      return true;
    },
    setSceneScopeFadersEnabled: (_sceneId, enabled) =>
      setAppState((state) =>
        updateSelectedSceneToggle(state, "faders", enabled),
      ),
    setSceneScopePanEnabled: (_sceneId, enabled) =>
      setAppState((state) => updateSelectedSceneToggle(state, "pan", enabled)),
  };

  return (
    <AppStateProvider
      appState={appState}
      commandError={props.commandError ?? null}
    >
      <AppCommandsProvider commands={commands}>
        {props.children}
      </AppCommandsProvider>
    </AppStateProvider>
  );
}

function updateSelectedSceneToggle(
  state: AppViewState,
  toggle: "faders" | "pan",
  enabled: boolean,
): AppViewState {
  const selectedSceneId = state.selectedSceneId;
  if (!selectedSceneId) return state;

  return {
    ...state,
    sceneConfigs: state.sceneConfigs.map((scene) =>
      scene.sceneId === selectedSceneId
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

export const SettingsPlaceholder: Story = {
  args: {
    activeTab: "settings",
  },
};

export const CommandError: Story = {
  args: {
    commandError: "Unable to save show file: permission denied.",
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
