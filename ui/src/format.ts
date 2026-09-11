import type { ChannelConfig, ChannelSummary } from "./types";

export function formatDb(value: number) {
  return `${value.toFixed(1)} dB`;
}

/**
 * @cc [owner:mixxorz,label:formatting] channel-name-exact-match
 * Channel lookup MUST match both numeric group and channel; absent coordinates MUST render
 * `Unknown` rather than borrowing a same-numbered channel from another group.
 */
export function channelName(
  channels: ChannelSummary[],
  group: number,
  channel: number,
) {
  return (
    channels.find((entry) => entry.group === group && entry.channel === channel)
      ?.name ?? "Unknown"
  );
}

/**
 * @cc [owner:mixxorz,label:formatting] lv1-group-display-mapping
 * LV1 group IDs MUST map as follows: 0 to `Inputs`, 1 to `Groups`, 2 to `Aux`, 3/4/5/7/8 to
 * `Masters`, 6 to `Matrix`, and 12 to `Link/DCAs`; every other ID MUST render as `Unknown`.
 */
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
  return [
    "Inputs",
    "Groups",
    "Aux",
    "Masters",
    "Matrix",
    "Link/DCAs",
    "Unknown",
  ].indexOf(groupName);
}

export function channelButtonLabel(group: number, channel: number) {
  if (group === 3) return "LR";
  if (group === 4) return "C";
  if (group === 5) return "M";
  if (group === 7) return "Cue";
  if (group === 8) return "TB";
  return String(channel + 1);
}

/**
 * @cc [owner:mixxorz,label:formatting] scene-number-placeholder
 * A nullish scene index MUST render `---`; otherwise the zero-based index MUST render as a
 * one-based number padded to at least three digits.
 */
export function formatSceneNumber(index: number | null | undefined): string {
  if (index === null || index === undefined) {
    return "---";
  }

  return String(index + 1).padStart(3, "0");
}

/**
 * @cc [owner:mixxorz,label:formatting] duration-one-decimal
 * Millisecond durations MUST render as seconds rounded to exactly one decimal place, including
 * trailing `.0` for whole-second and zero values.
 */
export function formatDurationSeconds(durationMs: number) {
  return (durationMs / 1000).toFixed(1);
}

export function formatSceneDurationSummary(durationMs: number) {
  return `${formatDurationSeconds(durationMs)}s`;
}

/**
 * @cc [owner:mixxorz,label:formatting] pan-family-nullability
 * The summary MUST include each non-null pan, balance, and width value in that order, including
 * numeric zero, and MUST render `No pan values` only when all three are nullish.
 */
export function formatPanFamilySummary(config: ChannelConfig) {
  const values = [
    config.pan == null ? null : `pan ${config.pan.toFixed(1)}`,
    config.balance == null ? null : `balance ${config.balance.toFixed(1)}`,
    config.width == null ? null : `width ${config.width.toFixed(1)}`,
  ].filter((value): value is string => value !== null);

  return values.length > 0 ? values.join(" · ") : "No pan values";
}
