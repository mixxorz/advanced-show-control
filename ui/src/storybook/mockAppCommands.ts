import type { AppCommands } from "../appContext";

const noop = () => {};
const promiseTrue = async () => true;

export const mockAppCommands: AppCommands = {
  abortAll: noop,
  addSceneToActiveCueList: noop,
  cueEntry: noop,
  copySceneSettings: noop,
  createCueList: noop,
  deleteCueList: noop,
  disconnect: noop,
  newShowFile: noop,
  openShowFile: noop,
  pasteSceneSettings: noop,
  removeCueEntry: noop,
  recallCuedCue: noop,
  renameCueList: noop,
  probeLv1TcpConnectLatency: async () => ({ tcpConnectMs: 3 }),
  reorderCueEntries: noop,
  reorderCueLists: noop,
  saveShowFile: noop,
  saveShowFileAs: noop,
  selectScene: noop,
  recallScene: noop,
  selectSystem: noop,
  setActiveCueList: noop,
  setAllChannelsScoped: noop,
  setChannelScoped: noop,
  setSceneDurationMs: promiseTrue,
  setSceneScopeFadersEnabled: noop,
  setSceneScopePanEnabled: noop,
  storeSceneConfig: promiseTrue,
  toggleLockout: noop,
};
