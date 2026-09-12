const APP_TITLE = "Advanced Show Control";

/**
 * @cc [owner:mixxorz,label:product;formatting] session-window-title-state
 * The title MUST use the projected show-file name with only its final extension removed, and MUST
 * append ` *` if and only if the projected session is dirty; it MUST NOT infer state from a path or
 * local save operation.
 */
export function formatSessionWindowTitle(showFileName: string, dirty: boolean) {
  const sessionName = showFileName.replace(/\.[^.]+$/, "");

  return `${APP_TITLE} - ${sessionName}${dirty ? " *" : ""}`;
}
