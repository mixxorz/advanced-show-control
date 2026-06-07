import type { ChannelSummary } from "./types";

export function formatDb(value: number) {
  return `${value.toFixed(1)} dB`;
}

export function channelName(channels: ChannelSummary[], group: number, channel: number) {
  return channels.find((entry) => entry.group === group && entry.channel === channel)?.name ?? "Unknown";
}

export function channelDisplayGroup(group: number) {
  if (group === 0) return "Inputs";
  if (group === 1) return "Groups";
  if (group === 2) return "Aux";
  if (group === 6) return "Matrix";
  if ([3, 4, 5, 7, 8].includes(group)) return "Masters";
  return "Unknown";
}

export function channelDisplayGroupOrder(groupName: string) {
  return ["Inputs", "Groups", "Aux", "Matrix", "Masters", "Unknown"].indexOf(groupName);
}

export function channelButtonLabel(group: number, channel: number) {
  if (group === 3) return "LR";
  if (group === 4) return "C";
  if (group === 5) return "Mono";
  if (group === 7) return "Cue";
  if (group === 8) return "TB";
  return String(channel);
}

export function formatDurationSeconds(durationMs: number) {
  return (durationMs / 1000).toFixed(1);
}
