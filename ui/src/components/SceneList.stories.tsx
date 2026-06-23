import type { Meta, StoryObj } from "@storybook/react-vite";
import { MockAppProviders } from "../storybook/MockAppProviders";
import {
  connectedAppState,
  connectedWithDuplicateScenesAppState,
} from "../storybook/mockAppState";
import {
  disconnectedAppViewState,
  type AppViewState,
  type SceneConfig,
} from "../types";
import { SceneListView } from "./SceneList";

type SceneListStoryArgs = {
  appState?: AppViewState;
  cuedSceneInternalId?: string | null;
};

const sceneNames = [
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

function makeSceneListConfig(index: number, name: string): SceneConfig {
  const source = connectedAppState.sceneConfigs[index % 2];

  return {
    ...source,
    internalSceneId: `scene-list-${index}`,
    sceneIndex: index,
    sceneName: name,
    durationMs: index === 0 ? 0 : (index % 6) * 500 + 1000,
  };
}

const manySceneConfigs = sceneNames.map((name, index) =>
  makeSceneListConfig(index, name),
);

const manyScenesAppState: AppViewState = {
  ...connectedAppState,
  cuedSceneInternalId: manySceneConfigs[5].internalSceneId,
  currentScene: { index: 2, name: "S01: The Wonderful Blood" },
  sceneConfigs: manySceneConfigs,
  selectedSceneInternalId: manySceneConfigs[6].internalSceneId,
};

const manyScenesActiveSelectedAppState: AppViewState = {
  ...manyScenesAppState,
  selectedSceneInternalId: manySceneConfigs[2].internalSceneId,
};

const meta: Meta<SceneListStoryArgs> = {
  title: "Scenes/Scene List/SceneList",
  decorators: [
    (Story) => (
      <main className="h-[32rem] w-[23rem] bg-console-bg p-6 text-console-primary">
        <Story />
      </main>
    ),
  ],
  args: {
    appState: manyScenesAppState,
    cuedSceneInternalId: manyScenesAppState.cuedSceneInternalId,
  },
  render: (args) => (
    <MockAppProviders appState={args.appState}>
      <SceneListView
        currentScene={args.appState?.currentScene ?? null}
        cuedSceneInternalId={args.cuedSceneInternalId}
        onRecallScene={() => {}}
        onSelectScene={() => {}}
        scenes={args.appState?.sceneConfigs ?? []}
        selectedSceneInternalId={args.appState?.selectedSceneInternalId ?? null}
      />
    </MockAppProviders>
  ),
};

export default meta;

type Story = StoryObj<SceneListStoryArgs>;

export const Populated: Story = {};

export const ActiveSelected: Story = {
  args: {
    appState: manyScenesActiveSelectedAppState,
  },
};

export const CuedSelected: Story = {
  args: {
    cuedSceneInternalId: manySceneConfigs[6].internalSceneId,
  },
};

export const DuplicateSceneWarning: Story = {
  args: {
    appState: connectedWithDuplicateScenesAppState,
  },
};

export const Empty: Story = {
  args: {
    appState: disconnectedAppViewState,
  },
};
