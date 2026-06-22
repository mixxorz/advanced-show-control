const APP_TITLE = "Advanced Show Control";

export function formatSessionWindowTitle(showFileName: string, dirty: boolean) {
  const sessionName = showFileName.replace(/\.[^.]+$/, "");

  return `${APP_TITLE} - ${sessionName}${dirty ? " *" : ""}`;
}
