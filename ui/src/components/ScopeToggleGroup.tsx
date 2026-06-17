import { ConsoleButton } from "./ConsoleButton";

export function ScopeToggleGroup(props: {
  fadersEnabled: boolean;
  panEnabled: boolean;
  onToggleFaders: () => void;
  onTogglePan: () => void;
}) {
  return (
    <div className="flex gap-2">
      <ConsoleButton
        active={props.fadersEnabled}
        onClick={props.onToggleFaders}
      >
        FADER
      </ConsoleButton>
      <ConsoleButton active={props.panEnabled} onClick={props.onTogglePan}>
        PAN
      </ConsoleButton>
    </div>
  );
}
