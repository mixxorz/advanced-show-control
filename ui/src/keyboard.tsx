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

type KeyboardContextValue = {
  registerHandler: (handler: KeyboardHandler) => () => void;
  shortcutCapture: ShortcutCaptureApi;
};

const KeyboardContext = createContext<KeyboardContextValue | null>(null);

export function KeyboardProvider(props: { children: ReactNode }) {
  const handlers = useRef(new Map<string, KeyboardHandler>());
  const activeCapture = useRef<ShortcutCaptureRequest | null>(null);
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

  const startCapture = useCallback((request: ShortcutCaptureRequest) => {
    activeCapture.current = request;
    setActiveCaptureId(request.id);
  }, []);

  useEffect(() => {
    return registerHandler({
      id: CAPTURE_HANDLER_ID,
      priority: CAPTURE_PRIORITY,
      handleKeyDown(event) {
        const current = activeCapture.current;
        if (!current) return "ignored";
        if (event.key === "Escape") {
          cancelCapture(current.id);
          return "handled";
        }
        if (isModifierKey(event.key)) {
          return "handled";
        }

        clearCapture();
        current.onCapture({
          key: normalizeCapturedKey(event),
          modifiers: event.modifiers,
        });
        return "handled";
      },
    });
  }, [cancelCapture, clearCapture, registerHandler]);

  useEffect(() => {
    function handleKeyDown(originalEvent: KeyboardEvent) {
      const appEvent = normalizeKeyboardEvent(originalEvent);
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
  }, []);

  const shortcutCapture = useMemo<ShortcutCaptureApi>(
    () => ({
      activeCaptureId,
      startCapture,
      cancelCapture,
      isCapturing: (id) => activeCaptureId === id,
    }),
    [activeCaptureId, cancelCapture, startCapture],
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

export function useKeyboardHandler(handler: KeyboardHandler) {
  const context = useKeyboardContext();
  useEffect(() => context.registerHandler(handler), [context, handler]);
}

export function useShortcutCapture() {
  return useKeyboardContext().shortcutCapture;
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
    originalEvent: event,
  };
}

function normalizeCapturedKey(event: AppKeyboardEvent) {
  const key = keyFromCode(event.code) ?? event.key;
  return key.length === 1 ? key.toUpperCase() : key;
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

function isModifierKey(key: string) {
  return (
    key === "Shift" || key === "Control" || key === "Alt" || key === "Meta"
  );
}

const CAPTURE_HANDLER_ID = "shortcut-capture";
const CAPTURE_PRIORITY = 1000;

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
