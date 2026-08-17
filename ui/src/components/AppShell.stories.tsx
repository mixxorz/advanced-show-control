import { useState, type ComponentProps } from "react";
import type { Meta, StoryObj } from "@storybook/react-vite";
import { expect, within } from "storybook/test";
import {
  AppCommandsProvider,
  AppStateProvider,
  type AppCommands,
} from "../appContext";
import { KeyboardProvider } from "../keyboard";
import { disconnectedAppViewState, type CueEntry } from "../types";
import {
  connectedAppState,
  cueListStateFixture,
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

const tuningSceneNames = [
  "Tuning: C",
  "Tuning: Db",
  "Tuning: D",
  "Tuning: Eb",
  "Tuning: E",
  "Tuning: F",
  "Tuning: Gb",
  "Tuning: G",
  "Tuning: Ab",
  "Tuning: A",
  "Tuning: Bb",
  "Tuning: B",
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

const shellSceneConfigs = [...shellSceneNames, ...tuningSceneNames].map(
  (name, index) => makeShellSceneConfig(index, name),
);
const serviceSceneConfigs = shellSceneConfigs.filter(
  (scene) => !scene.sceneName.startsWith("Tuning:"),
);
const tuningSceneConfigs = shellSceneConfigs.filter((scene) =>
  scene.sceneName.startsWith("Tuning:"),
);

const sceneTabAppState: AppViewState = {
  ...connectedAppState,
  currentScene: { index: 1, name: "S01: The Wonderful Blood" },
  sceneConfigs: shellSceneConfigs,
  selectedSceneInternalId: shellSceneConfigs[6].internalSceneId,
};

const cueListsTabAppState: AppViewState = {
  ...cueListStateFixture,
  currentScene: { index: 1, name: "S01: The Wonderful Blood" },
  sceneConfigs: shellSceneConfigs,
  sceneCount: shellSceneConfigs.length,
  selectedSceneInternalId: null,
  cueLists: [
    {
      id: "cue-list-service",
      name: "Service",
      entries: serviceSceneConfigs.slice(0, 8).map((scene, index) => ({
        id: `service-cue-${index + 1}`,
        sceneInternalId: scene.internalSceneId,
      })),
    },
    {
      id: "cue-list-tuning",
      name: "Tuning",
      entries: tuningSceneConfigs.map((scene, index) => ({
        id: `tuning-cue-${index + 1}`,
        sceneInternalId: scene.internalSceneId,
      })),
    },
    {
      id: "cue-list-rehearsal",
      name: "Rehearsal",
      entries: serviceSceneConfigs.slice(2, 6).map((scene, index) => ({
        id: `rehearsal-cue-${index + 1}`,
        sceneInternalId: scene.internalSceneId,
      })),
    },
  ],
  activeCueListId: "cue-list-service",
  cuedCueEntryId: "service-cue-3",
};

const offlineSceneTabAppState: AppViewState = {
  ...sceneTabAppState,
  connection: "disconnected",
  connectedLv1Identity: null,
  currentScene: null,
  discoveredLv1Systems: discoveredSystemsAppState.discoveredLv1Systems,
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

  const updateAppState = async (update: Parameters<typeof setAppState>[0]) => {
    setAppState(update);
  };

  const commands: AppCommands = {
    ...mockAppCommands,
    addSceneToActiveCueList: (sceneInternalId, insertIndex) =>
      updateAppState((state) => {
        const activeCueListId = state.activeCueListId;
        if (!activeCueListId) return state;

        return {
          ...state,
          cueLists: state.cueLists.map((cueList) => {
            if (cueList.id !== activeCueListId) return cueList;

            const entries = [...cueList.entries];
            entries.splice(clampInsertIndex(insertIndex, entries.length), 0, {
              id: `story-cue-${Date.now()}`,
              sceneInternalId,
            });
            return { ...cueList, entries };
          }),
        };
      }),
    cueEntry: (cueEntryId) =>
      updateAppState((state) => ({ ...state, cuedCueEntryId: cueEntryId })),
    recallScene: (internalSceneId) =>
      updateAppState((state) => {
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
      updateAppState((state) => ({
        ...state,
        selectedSceneInternalId: internalSceneId,
      })),
    setAllChannelsScoped: (_internalSceneId, scoped) =>
      updateAppState((state) => {
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
      updateAppState((state) => {
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
      updateAppState((state) => {
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
      updateAppState((state) =>
        updateSelectedSceneToggle(state, "faders", enabled),
      ),
    setSceneScopePanEnabled: (_internalSceneId, enabled) =>
      updateAppState((state) =>
        updateSelectedSceneToggle(state, "pan", enabled),
      ),
    removeCueEntry: (cueEntryId) =>
      updateAppState((state) => ({
        ...state,
        cueLists: state.cueLists.map((cueList) => ({
          ...cueList,
          entries: cueList.entries.filter((entry) => entry.id !== cueEntryId),
        })),
        cuedCueEntryId:
          state.cuedCueEntryId === cueEntryId ? null : state.cuedCueEntryId,
      })),
    reorderCueEntries: (orderedIds) =>
      updateAppState((state) => {
        const activeCueListId = state.activeCueListId;
        if (!activeCueListId) return state;

        return {
          ...state,
          cueLists: state.cueLists.map((cueList) =>
            cueList.id === activeCueListId
              ? {
                  ...cueList,
                  entries: orderCueEntries(cueList.entries, orderedIds),
                }
              : cueList,
          ),
        };
      }),
    reorderCueLists: (orderedIds) =>
      updateAppState((state) => ({
        ...state,
        cueLists: orderCueLists(state.cueLists, orderedIds),
      })),
    setActiveCueList: (cueListId) =>
      updateAppState((state) => ({
        ...state,
        activeCueListId: cueListId,
        cuedCueEntryId: null,
      })),
    toggleLockout: () =>
      updateAppState((state) => ({ ...state, lockout: !state.lockout })),
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

function clampInsertIndex(insertIndex: number, length: number) {
  return Math.max(0, Math.min(insertIndex, length));
}

function orderCueEntries(entries: CueEntry[], orderedIds: string[]) {
  const entriesById = new Map(entries.map((entry) => [entry.id, entry]));
  const orderedEntries = orderedIds
    .map((id) => entriesById.get(id))
    .filter((entry): entry is CueEntry => Boolean(entry));
  const orderedIdSet = new Set(orderedIds);
  return [
    ...orderedEntries,
    ...entries.filter((entry) => !orderedIdSet.has(entry.id)),
  ];
}

function orderCueLists(
  cueLists: AppViewState["cueLists"],
  orderedIds: string[],
) {
  const cueListsById = new Map(
    cueLists.map((cueList) => [cueList.id, cueList]),
  );
  const orderedCueLists = orderedIds
    .map((id) => cueListsById.get(id))
    .filter((cueList): cueList is AppViewState["cueLists"][number] =>
      Boolean(cueList),
    );
  const orderedIdSet = new Set(orderedIds);
  return [
    ...orderedCueLists,
    ...cueLists.filter((cueList) => !orderedIdSet.has(cueList.id)),
  ];
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
      canvas.getByRole("heading", { name: "Scene library" }),
    ).toBeInTheDocument();
    await expect(
      canvas.getByRole("button", { name: "Scenes" }),
    ).toBeInTheDocument();
    await expect(
      canvas.getByRole("button", { name: "Settings" }),
    ).toBeInTheDocument();
  },
};

export const CueListsTab: Story = {
  args: {
    activeTab: "cue-lists",
    appState: cueListsTabAppState,
  },
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);

    await expect(
      canvas.getByRole("heading", { name: "Scene library" }),
    ).toBeInTheDocument();
    await expect(
      canvas.getByRole("heading", { name: "Service" }),
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

export const EmptyMainShell: Story = {
  args: {
    appState: disconnectedAppViewState,
  },
};
