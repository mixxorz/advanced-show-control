import { ConsoleButton } from "./ConsoleButton";

export function ScopeToggleGroup(props: {
  fadersEnabled: boolean;
  panEnabled: boolean;
  onToggleFaders: () => void;
  onTogglePan: () => void;
  size?: "default" | "small";
}) {
  return (
    <div className="flex gap-2">
      <ConsoleButton
        active={props.fadersEnabled}
        onClick={props.onToggleFaders}
        size={props.size}
      >
        FADER
      </ConsoleButton>
      <ConsoleButton
        active={props.panEnabled}
        onClick={props.onTogglePan}
        size={props.size}
      >
        PAN
      </ConsoleButton>
    </div>
  );
}
