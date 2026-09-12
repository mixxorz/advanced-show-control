import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));

import { invoke } from "@tauri-apps/api/core";
import {
  abortAll,
  copySceneSettings,
  createCueList,
  deleteCueList,
  disconnectLv1,
  frontendReady,
  newShowFile,
  pasteSceneSettings,
  probeLv1TcpConnectLatency,
  recallCuedCue,
  recallScene,
  reorderCueEntries,
  reorderCueLists,
  replaceAppSettings,
  selectSceneConfig,
  setChannelScoped,
} from "./commands";

beforeEach(() => {
  vi.clearAllMocks();
});

describe("production command bridge", () => {
  it.each([
    ["frontend_ready", frontendReady],
    ["abort_all_fades", abortAll],
    ["disconnect_lv1", disconnectLv1],
    ["new_show_file", newShowFile],
  ])("invokes %s without arguments", async (commandName, command) => {
    vi.mocked(invoke).mockResolvedValue(undefined);

    await command();

    expect(invoke).toHaveBeenCalledWith(commandName);
  });

  it("forwards scene identifiers", async () => {
    vi.mocked(invoke).mockResolvedValue(undefined);

    await recallScene("scene-1");
    await selectSceneConfig("scene-2");

    expect(invoke).toHaveBeenNthCalledWith(1, "recall_scene", {
      internalSceneId: "scene-1",
    });
    expect(invoke).toHaveBeenNthCalledWith(2, "select_scene_config", {
      internalSceneId: "scene-2",
    });
  });

  it("forwards channel scope arguments", async () => {
    vi.mocked(invoke).mockResolvedValue(undefined);

    await setChannelScoped("scene-1", 2, 7, true);

    expect(invoke).toHaveBeenCalledWith("set_channel_scoped", {
      internalSceneId: "scene-1",
      group: 2,
      channel: 7,
      scoped: true,
    });
  });
});

describe("probeLv1TcpConnectLatency", () => {
  it("omits timeoutMs when not provided", async () => {
    vi.mocked(invoke).mockResolvedValue({ tcpConnectMs: 5 });

    await probeLv1TcpConnectLatency({
      uuid: "lv1-demo",
      host: "FOH LV1",
      address: "192.168.1.42",
      port: 22000,
    });

    expect(invoke).toHaveBeenCalledWith("probe_lv1_tcp_connect_latency", {
      identity: {
        uuid: "lv1-demo",
        host: "FOH LV1",
        address: "192.168.1.42",
        port: 22000,
      },
    });
  });

  it("forwards timeoutMs when provided", async () => {
    vi.mocked(invoke).mockResolvedValue({ tcpConnectMs: 5 });

    await probeLv1TcpConnectLatency(
      {
        uuid: "lv1-demo",
        host: "FOH LV1",
        address: "192.168.1.42",
        port: 22000,
      },
      750,
    );

    expect(invoke).toHaveBeenCalledWith("probe_lv1_tcp_connect_latency", {
      identity: {
        uuid: "lv1-demo",
        host: "FOH LV1",
        address: "192.168.1.42",
        port: 22000,
      },
      timeoutMs: 750,
    });
  });
});

describe("cue list commands", () => {
  it("calls create_cue_list with the provided name", async () => {
    vi.mocked(invoke).mockResolvedValue(undefined);

    await createCueList("Main");

    expect(invoke).toHaveBeenCalledWith("create_cue_list", { name: "Main" });
  });

  it("calls delete_cue_list with the provided id", async () => {
    vi.mocked(invoke).mockResolvedValue(undefined);

    await deleteCueList("cue-list-1");

    expect(invoke).toHaveBeenCalledWith("delete_cue_list", {
      cueListId: "cue-list-1",
    });
  });

  it("calls reorder_cue_lists with the provided ids", async () => {
    vi.mocked(invoke).mockResolvedValue(undefined);

    await reorderCueLists(["a", "b", "c"]);

    expect(invoke).toHaveBeenCalledWith("reorder_cue_lists", {
      orderedIds: ["a", "b", "c"],
    });
  });

  it("forwards the complete cue-entry order", async () => {
    vi.mocked(invoke).mockResolvedValue(undefined);

    await reorderCueEntries(["entry-c", "entry-a", "entry-b"]);

    expect(invoke).toHaveBeenCalledWith("reorder_cue_entries", {
      orderedEntryIds: ["entry-c", "entry-a", "entry-b"],
    });
  });

  it("calls recall_cued_cue without arguments", async () => {
    vi.mocked(invoke).mockResolvedValue(undefined);

    await recallCuedCue();

    expect(invoke).toHaveBeenCalledWith("recall_cued_cue");
  });
});

describe("settings commands", () => {
  it("forwards one complete settings replacement", async () => {
    vi.mocked(invoke).mockResolvedValue(undefined);
    const settings = {
      autoLoadLastShowFile: true,
      autoSaveSessions: false,
      keyboardShortcuts: {
        go: {
          key: "Space",
          modifiers: {
            shift: false,
            control: false,
            alt: false,
            meta: false,
          },
        },
        cue: {
          key: "C",
          modifiers: {
            shift: false,
            control: false,
            alt: false,
            meta: false,
          },
        },
      },
      timeDisplay: "twentyFourHour" as const,
      faderOverrideSensitivity: 9,
      enableExtensiveDiagnostics: false,
      sameSceneRecallEnabled: true,
      sameSceneRecallThresholdMs: 500,
    };

    await replaceAppSettings(settings);

    expect(invoke).toHaveBeenCalledWith("replace_app_settings", { settings });
  });
});

describe("scene settings clipboard commands", () => {
  it("calls copy_scene_settings with the provided internal scene id", async () => {
    vi.mocked(invoke).mockResolvedValue(undefined);

    await copySceneSettings("scene-1");

    expect(invoke).toHaveBeenCalledWith("copy_scene_settings", {
      internalSceneId: "scene-1",
    });
  });

  it("calls paste_scene_settings with the provided internal scene id", async () => {
    vi.mocked(invoke).mockResolvedValue(undefined);

    await pasteSceneSettings("scene-1");

    expect(invoke).toHaveBeenCalledWith("paste_scene_settings", {
      internalSceneId: "scene-1",
    });
  });
});
