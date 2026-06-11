import type { ChannelConfig, ChannelSummary } from "./types";

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
  if (group === 12) return "Link/DCAs";
  if ([3, 4, 5, 7, 8].includes(group)) return "Masters";
  return "Unknown";
}

export function channelDisplayGroupOrder(groupName: string) {
  return ["Inputs", "Groups", "Aux", "Matrix", "Masters", "Link/DCAs", "Unknown"].indexOf(groupName);
}

export function channelButtonLabel(group: number, channel: number) {
  if (group === 3) return "LR";
  if (group === 4) return "C";
  if (group === 5) return "Mono";
  if (group === 7) return "Cue";
  if (group === 8) return "TB";
  return String(channel);
}

export function formatSceneNumber(index: number | null | undefined): string {
  if (index === null || index === undefined) {
    return "--";
  }

  return String(index + 1);
}

export function formatDurationSeconds(durationMs: number) {
  return (durationMs / 1000).toFixed(1);
}

export function formatSceneDurationSummary(durationMs: number) {
  return durationMs === 0 ? "Immediate" : `${durationMs} ms`;
}

export function formatPanFamilySummary(config: ChannelConfig) {
  const values = [
    config.pan == null ? null : `pan ${config.pan.toFixed(1)}`,
    config.balance == null ? null : `balance ${config.balance.toFixed(1)}`,
    config.width == null ? null : `width ${config.width.toFixed(1)}`,
  ].filter((value): value is string => value !== null);

  return values.length > 0 ? values.join(" · ") : "No pan values";
}
