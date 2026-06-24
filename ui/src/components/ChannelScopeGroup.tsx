import type { ChannelConfig } from "../types";
import { ChannelScopeButton } from "./ChannelScopeButton";

export function ChannelScopeGroup(props: {
  configs: ChannelConfig[];
  groupName: string;
  internalSceneId: string;
  scoped: Set<string>;
}) {
  return (
    <section>
      <h4 className="text-xs uppercase tracking-[0.08em] text-console-secondary">
        {props.groupName}
      </h4>
      <div className="mt-2 flex flex-wrap gap-1.5">
        {[...props.configs]
          .sort((a, b) => a.channel - b.channel)
          .map((config) => {
            const key = `${config.group}:${config.channel}`;
            const isScoped = props.scoped.has(key);

            return (
              <ChannelScopeButton
                config={config}
                key={key}
                internalSceneId={props.internalSceneId}
                scoped={isScoped}
              />
            );
          })}
      </div>
    </section>
  );
}
