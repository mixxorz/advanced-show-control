import type { AppCommands } from "../appContext";

const noop = () => {};
const promiseTrue = async () => true;

export const mockAppCommands: AppCommands = {
  abortAll: noop,
  disconnect: noop,
  newShowFile: noop,
  openShowFile: noop,
  saveShowFile: noop,
  saveShowFileAs: noop,
  selectScene: noop,
  selectSystem: noop,
  setAllChannelsScoped: noop,
  setChannelScoped: noop,
  setSceneDurationMs: promiseTrue,
  setSceneScopeFadersEnabled: noop,
  setSceneScopePanEnabled: noop,
  storeSceneConfig: promiseTrue,
  toggleLockout: noop,
};
