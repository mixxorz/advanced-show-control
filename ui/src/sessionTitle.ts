const APP_TITLE = "Advanced Show Control";

export function formatSessionWindowTitle(showFileName: string, dirty: boolean) {
  return `${APP_TITLE} - ${showFileName}${dirty ? " *" : ""}`;
}
