import type { ChannelSummary } from "./types";

export function formatDb(value: number) {
  return `${value.toFixed(1)} dB`;
}

export function channelName(channels: ChannelSummary[], group: number, channel: number) {
  return channels.find((entry) => entry.group === group && entry.channel === channel)?.name ?? "Unknown";
}

export function formatDurationSeconds(durationMs: number) {
  return (durationMs / 1000).toFixed(1);
}
