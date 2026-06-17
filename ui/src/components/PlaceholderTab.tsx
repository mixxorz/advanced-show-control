import { Panel } from "./Panel";

export function PlaceholderTab(props: { name: string }) {
  return (
    <Panel className="grid min-h-[32rem] place-items-center p-8">
      <div className="max-w-lg text-center">
        <h2 className="text-xl font-semibold uppercase tracking-[0.08em] text-console-primary">
          {props.name}
        </h2>
        <p className="mt-3 text-console-secondary">
          This workflow is part of the product shell but is not built yet.
        </p>
      </div>
    </Panel>
  );
}
