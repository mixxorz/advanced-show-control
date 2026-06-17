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
  cuedSceneId?: string | null;
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
    sceneId: `scene-list-${index}`,
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
  cuedSceneId: manySceneConfigs[5].sceneId,
  currentScene: { index: 2, name: "S01: The Wonderful Blood" },
  sceneConfigs: manySceneConfigs,
  selectedSceneId: manySceneConfigs[6].sceneId,
};

const manyScenesActiveSelectedAppState: AppViewState = {
  ...manyScenesAppState,
  selectedSceneId: manySceneConfigs[2].sceneId,
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
    cuedSceneId: manyScenesAppState.cuedSceneId,
  },
  render: (args) => (
    <MockAppProviders appState={args.appState}>
      <SceneListView
        currentScene={args.appState?.currentScene ?? null}
        cuedSceneId={args.cuedSceneId}
        onRecallScene={() => {}}
        onSelectScene={() => {}}
        scenes={args.appState?.sceneConfigs ?? []}
        selectedSceneId={args.appState?.selectedSceneId ?? null}
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
    cuedSceneId: manySceneConfigs[6].sceneId,
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
