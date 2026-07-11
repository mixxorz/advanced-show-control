# Logs

Use **Logs** to understand what the application has done and why an operation needs attention. The screen shows connection progress, completed operations, visible safety blocks, recoverable warnings, and failures that prevented a requested action.

![A populated Logs screen.](assets/screenshots/logs.png)

## Review An Operation

1. Open **Logs** after a connection, store, recall, cue, or file operation.
2. Read the newest message near the time of the event.
3. Use its severity to decide whether to continue, correct a condition, or collect support information.
4. Follow the linked procedure before changing scene configuration or console state.

If no entries are available, the screen displays **No frontend logs yet.**

## Read The Entries

| Field | Meaning |
| --- | --- |
| Timestamp | The time the entry appeared in the Logs screen. |
| Severity | `INFO`, `WARNING`, or `ERROR`. |
| Message | A complete description of the event for the operator. |

An `INFO` entry records an operating fact or completed action. A `WARNING` entry identifies a visible safety block or recoverable problem. An `ERROR` entry identifies a command or file operation that could not complete. If you see a warning or error before a recall, correct the stated condition and verify the intended scene before you try again.

## Diagnostic Files

The Logs screen is the operational view, not a complete diagnostic history. It does not display `DEBUG` entries. Use the diagnostic JSONL file when a support investigation needs more detail.

On macOS, normal application diagnostic files are written under:

```text
~/Library/Application Support/com.advancedshowcontrol.app/logs/diagnostics-*.jsonl
```

Each application run creates a timestamped file. Enable **Extensive diagnostics** in [Settings](settings.md#general-settings) when you need `DEBUG` entries in that file. Disable it after investigation because the files can grow quickly.

## Report A Problem

Include these details when you report a problem:

- Application version and operating system version.
- Approximate time of the problem and the steps that reproduce it.
- Connection state, selected scene, and whether **SAFE** was active.
- Relevant Logs-screen entries or a short, relevant diagnostic-file excerpt.

Do not share a full `.ascs` session or complete diagnostic file by default. These files can contain show names, scene names, channel information, and other console details. Remove sensitive information from an excerpt, and provide a complete file only through a trusted support process that specifically requests it.

## Troubleshooting

For an operation with no useful visible entry, see [No Visible Log Explains A Failure](troubleshooting.md#no-visible-log-explains-a-failure).
