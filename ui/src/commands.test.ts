import { describe, expect, it, vi } from "vitest";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));

import { invoke } from "@tauri-apps/api/core";
import {
  createCueList,
  deleteCueList,
  probeLv1TcpConnectLatency,
  recallCuedCue,
  reorderCueLists,
} from "./commands";

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

  it("calls recall_cued_cue without arguments", async () => {
    vi.mocked(invoke).mockResolvedValue(undefined);

    await recallCuedCue();

    expect(invoke).toHaveBeenCalledWith("recall_cued_cue");
  });
});
