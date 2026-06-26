import { describe, expect, it, vi } from "vitest";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));

import { invoke } from "@tauri-apps/api/core";
import { probeLv1TcpConnectLatency } from "./commands";

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
