export function ShowFileControls(props: {
  dirty: boolean;
  fileName: string;
  filePath: string | null;
  onNew: () => void;
  onOpen: () => void;
  onSave: () => void;
  onSaveAs: () => void;
}) {
  return (
    <div className="rounded-xl border border-slate-800 bg-slate-950/60 px-4 py-3">
      <div className="text-sm font-semibold text-slate-100">
        {props.fileName}
        {props.dirty ? " *" : ""}
      </div>
      <div className="mt-1 text-xs text-slate-400">
        {props.filePath ?? "No show file saved"}
      </div>
      <div className="mt-3 flex flex-wrap gap-2">
        <button
          className="rounded border border-slate-700 px-3 py-1 text-sm text-slate-100 hover:bg-slate-800"
          onClick={props.onNew}
        >
          New
        </button>
        <button
          className="rounded border border-slate-700 px-3 py-1 text-sm text-slate-100 hover:bg-slate-800"
          onClick={props.onOpen}
        >
          Open
        </button>
        <button
          className="rounded border border-slate-700 px-3 py-1 text-sm text-slate-100 hover:bg-slate-800"
          onClick={props.onSave}
        >
          Save
        </button>
        <button
          className="rounded border border-slate-700 px-3 py-1 text-sm text-slate-100 hover:bg-slate-800"
          onClick={props.onSaveAs}
        >
          Save As
        </button>
      </div>
    </div>
  );
}
