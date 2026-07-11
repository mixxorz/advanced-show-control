# Logs

Use **Logs** to check connection progress, completed actions, safety blocks, and failures.

![A populated Logs screen.](assets/screenshots/logs.png)

Each entry shows a timestamp, severity, and message. `INFO` records an action or status change. `WARNING` identifies a condition you should correct. `ERROR` identifies an action that could not complete.

**No frontend logs yet.** means no messages have appeared in Logs during this session.

## Diagnostic Files

Logs does not show `DEBUG` messages. Enable **Extensive diagnostics** when you need detailed diagnostic messages, then disable it after investigation because files can grow quickly.

On macOS, diagnostic files are written under:

```text
~/Library/Application Support/com.advancedshowcontrol.app/logs/diagnostics-*.jsonl
```

When you report a problem, include the app version, operating system, approximate time, connection state, selected scene, SAFE state, and relevant log lines. Do not share a complete session or diagnostic file unless a trusted support process asks for it; those files can contain show and console information.
