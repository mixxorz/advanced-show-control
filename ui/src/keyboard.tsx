/* eslint-disable react-refresh/only-export-components */
import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useRef,
  useState,
  type ReactNode,
} from "react";
import type { KeyboardShortcut, KeyboardShortcutModifiers } from "./types";

export type AppKeyboardEvent = {
  code: string;
  key: string;
  modifiers: KeyboardShortcutModifiers;
  repeat: boolean;
  originalEvent: KeyboardEvent;
};

export type KeyboardHandler = {
  id: string;
  priority: number;
  enabled?: boolean;
  handleKeyDown: (event: AppKeyboardEvent) => "handled" | "ignored";
};

export type ShortcutCaptureRequest = {
  id: string;
  onCapture: (shortcut: KeyboardShortcut) => void;
  onCancel?: () => void;
};

export type ShortcutCaptureApi = {
  activeCaptureId: string | null;
  startCapture: (request: ShortcutCaptureRequest) => void;
  cancelCapture: (id?: string) => void;
  isCapturing: (id: string) => boolean;
};

type ActiveShortcutCaptureRequest = ShortcutCaptureRequest & {
  ownerId?: string;
};

type ShortcutCaptureContextApi = ShortcutCaptureApi & {
  startCaptureForOwner: (
    ownerId: string,
    request: ShortcutCaptureRequest,
  ) => void;
  cancelCaptureForOwner: (ownerId: string) => void;
};

type KeyboardContextValue = {
  registerHandler: (handler: KeyboardHandler) => () => void;
  shortcutCapture: ShortcutCaptureContextApi;
};

const KeyboardContext = createContext<KeyboardContextValue | null>(null);

/**
 * @cc [owner:mixxorz,label:keyboard] prioritized-key-routing
 * Each enabled handler MUST receive a normalized keydown in descending priority until one returns
 * `handled`; then native default behavior and propagation MUST be stopped and lower handlers MUST
 * not run. Disabled handlers MUST not receive the event.
 */
/**
 * @cc [owner:mixxorz,label:keyboard] capture-preempts-actions
 * Active shortcut capture MUST be routed before every externally registered handler regardless of
 * numeric priority, consume modifier-only keys without ending capture, cancel on Escape, and
 * capture exactly one non-modifier key before clearing itself.
 */
export function KeyboardProvider(props: { children: ReactNode }) {
  const handlers = useRef(new Map<string, KeyboardHandler>());
  const activeCapture = useRef<ActiveShortcutCaptureRequest | null>(null);
  const [activeCaptureId, setActiveCaptureId] = useState<string | null>(null);

  const registerHandler = useCallback((handler: KeyboardHandler) => {
    handlers.current.set(handler.id, handler);
    return () => {
      if (handlers.current.get(handler.id) === handler) {
        handlers.current.delete(handler.id);
      }
    };
  }, []);

  const clearCapture = useCallback(() => {
    activeCapture.current = null;
    setActiveCaptureId(null);
  }, []);

  const cancelCapture = useCallback(
    (id?: string) => {
      const current = activeCapture.current;
      if (!current || (id && current.id !== id)) return;
      clearCapture();
      current.onCancel?.();
    },
    [clearCapture],
  );

  const startCapture = useCallback((request: ActiveShortcutCaptureRequest) => {
    activeCapture.current = request;
    setActiveCaptureId(request.id);
  }, []);

  const startCaptureForOwner = useCallback(
    (ownerId: string, request: ShortcutCaptureRequest) => {
      startCapture({ ...request, ownerId });
    },
    [startCapture],
  );

  const cancelCaptureForOwner = useCallback(
    (ownerId: string) => {
      if (activeCapture.current?.ownerId === ownerId) {
        cancelCapture();
      }
    },
    [cancelCapture],
  );

  useEffect(() => {
    function handleKeyDown(originalEvent: KeyboardEvent) {
      const appEvent = normalizeKeyboardEvent(originalEvent);
      const capture = activeCapture.current;
      if (capture) {
        if (appEvent.key === "Escape") {
          cancelCapture(capture.id);
        } else if (!isModifierKey(appEvent.key)) {
          clearCapture();
          capture.onCapture({
            key: shortcutKeyFromEvent(appEvent),
            modifiers: appEvent.modifiers,
          });
        }
        originalEvent.preventDefault();
        originalEvent.stopPropagation();
        return;
      }

      const sortedHandlers = [...handlers.current.values()]
        .filter((handler) => handler.enabled !== false)
        .sort((a, b) => b.priority - a.priority);

      for (const handler of sortedHandlers) {
        if (handler.handleKeyDown(appEvent) === "handled") {
          originalEvent.preventDefault();
          originalEvent.stopPropagation();
          break;
        }
      }
    }

    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [cancelCapture, clearCapture]);

  const shortcutCapture = useMemo<ShortcutCaptureContextApi>(
    () => ({
      activeCaptureId,
      startCapture,
      startCaptureForOwner,
      cancelCapture,
      cancelCaptureForOwner,
      isCapturing: (id) => activeCaptureId === id,
    }),
    [
      activeCaptureId,
      cancelCapture,
      cancelCaptureForOwner,
      startCapture,
      startCaptureForOwner,
    ],
  );

  const value = useMemo(
    () => ({ registerHandler, shortcutCapture }),
    [registerHandler, shortcutCapture],
  );

  return (
    <KeyboardContext.Provider value={value}>
      {props.children}
    </KeyboardContext.Provider>
  );
}

/**
 * @cc [owner:mixxorz,label:keyboard] handler-registration-lifetime
 * A handler MUST be registered only while its hook instance is mounted, and replacing or unmounting
 * that instance MUST NOT unregister a newer handler that reused the same ID.
 */
export function useKeyboardHandler(handler: KeyboardHandler) {
  const context = useKeyboardContext();
  useEffect(() => context.registerHandler(handler), [context, handler]);
}

/**
 * @cc [owner:mixxorz,label:keyboard] capture-owner-cleanup
 * When an owner ID is supplied, unmount MUST cancel only that owner's active capture and MUST NOT
 * cancel a capture that has since been replaced by another owner.
 */
export function useShortcutCapture(ownerId?: string) {
  const shortcutCapture = useKeyboardContext().shortcutCapture;
  const { cancelCaptureForOwner } = shortcutCapture;

  useEffect(() => {
    if (!ownerId) return;
    return () => cancelCaptureForOwner(ownerId);
  }, [cancelCaptureForOwner, ownerId]);

  return useMemo<ShortcutCaptureApi>(() => {
    if (!ownerId) return shortcutCapture;
    return {
      ...shortcutCapture,
      startCapture: (request) =>
        shortcutCapture.startCaptureForOwner(ownerId, request),
    };
  }, [ownerId, shortcutCapture]);
}

function useKeyboardContext() {
  const context = useContext(KeyboardContext);
  if (!context) {
    throw new Error("KeyboardProvider is missing");
  }
  return context;
}

function normalizeKeyboardEvent(event: KeyboardEvent): AppKeyboardEvent {
  return {
    code: event.code,
    key: event.key,
    modifiers: {
      shift: event.shiftKey,
      control: event.ctrlKey,
      alt: event.altKey,
      meta: event.metaKey,
    },
    repeat: event.repeat,
    originalEvent: event,
  };
}

/**
 * @cc [owner:mixxorz,label:keyboard] shortcut-key-normalization
 * Letter and digit physical codes MUST normalize independently of keyboard layout or Shift glyph;
 * mapped punctuation uses its stable label, unknown printable physical keys use `code`, and only
 * then may `key` be used. Single-character results MUST be uppercased.
 */
export function shortcutKeyFromEvent(event: AppKeyboardEvent) {
  const key =
    keyFromCode(event.code) ?? printableCodeFallback(event) ?? event.key;
  return key.length === 1 ? key.toUpperCase() : key;
}

export function shortcutKeysEqual(left: string, right: string) {
  return left.toUpperCase() === right.toUpperCase();
}

/**
 * @cc [owner:mixxorz,label:keyboard] shortcut-exact-modifiers
 * Matching MUST be case-insensitive for the normalized physical key or persisted character key and
 * MUST require exact equality of all four modifier flags.
 */
export function shortcutMatchesEvent(
  shortcut: KeyboardShortcut,
  event: AppKeyboardEvent,
) {
  return (
    (shortcutKeysEqual(shortcut.key, shortcutKeyFromEvent(event)) ||
      shortcutKeysEqual(shortcut.key, event.key)) &&
    shortcut.modifiers.shift === event.modifiers.shift &&
    shortcut.modifiers.control === event.modifiers.control &&
    shortcut.modifiers.alt === event.modifiers.alt &&
    shortcut.modifiers.meta === event.modifiers.meta
  );
}

/**
 * @cc [owner:mixxorz,label:safety;keyboard] action-shortcut-interaction-block
 * Action shortcuts MUST be blocked whenever any ARIA modal is open or the event target is within an
 * editable control, select, contenteditable region, or dialog; ordinary non-dialog controls remain
 * eligible for routing.
 */
export function isActionShortcutBlocked(event: AppKeyboardEvent) {
  if (document.querySelector('[aria-modal="true"]') !== null) {
    return true;
  }

  const target = event.originalEvent.target;
  return (
    target instanceof Element &&
    target.closest(
      'input, textarea, select, [contenteditable]:not([contenteditable="false"]), [role="dialog"]',
    ) !== null
  );
}

function keyFromCode(code: string) {
  if (code.startsWith("Key") && code.length === 4) {
    return code.slice(3);
  }
  if (code.startsWith("Digit") && code.length === 6) {
    return code.slice(5);
  }
  return CODE_KEY_LABELS[code];
}

function printableCodeFallback(event: AppKeyboardEvent) {
  if (event.code && event.key.length === 1) {
    return event.code;
  }
  return undefined;
}

function isModifierKey(key: string) {
  return (
    key === "Shift" || key === "Control" || key === "Alt" || key === "Meta"
  );
}

const CODE_KEY_LABELS: Record<string, string> = {
  Backquote: "`",
  Backslash: "\\",
  BracketLeft: "[",
  BracketRight: "]",
  Comma: ",",
  Equal: "=",
  Minus: "-",
  Period: ".",
  Quote: "'",
  Semicolon: ";",
  Slash: "/",
};
