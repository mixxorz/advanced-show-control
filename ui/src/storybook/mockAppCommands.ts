import type { AppCommands } from "../appContext";

const mutationCompleted = async () => {};
const mutationSucceeded = async () => true;

export const mockAppCommands: AppCommands = {
  abortAll: mutationCompleted,
  addSceneToActiveCueList: mutationCompleted,
  cueEntry: mutationCompleted,
  copySceneSettings: mutationCompleted,
  createCueList: mutationCompleted,
  deleteCueList: mutationCompleted,
  disconnect: mutationCompleted,
  newShowFile: mutationCompleted,
  openShowFile: mutationCompleted,
  pasteSceneSettings: mutationCompleted,
  removeCueEntry: mutationCompleted,
  recallCuedCue: mutationCompleted,
  renameCueList: mutationCompleted,
  probeLv1TcpConnectLatency: async () => ({ tcpConnectMs: 3 }),
  reorderCueEntries: mutationCompleted,
  reorderCueLists: mutationCompleted,
  saveShowFile: mutationCompleted,
  saveShowFileAs: mutationCompleted,
  selectScene: mutationCompleted,
  recallScene: mutationCompleted,
  selectSystem: mutationCompleted,
  setActiveCueList: mutationCompleted,
  setAllChannelsScoped: mutationCompleted,
  setChannelScoped: mutationCompleted,
  setSceneDurationMs: mutationSucceeded,
  setSceneScopeFadersEnabled: mutationCompleted,
  setSceneScopePanEnabled: mutationCompleted,
  storeSceneConfig: mutationSucceeded,
  linkSceneConfig: mutationCompleted,
  deleteSceneConfig: mutationCompleted,
  toggleLockout: mutationCompleted,
};
