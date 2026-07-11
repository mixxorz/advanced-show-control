# Terminology

Use these terms consistently when you configure a fade or prepare a cue list.

| Term | Definition |
| --- | --- |
| LV1 scene | A scene created and owned by Waves eMotion LV1 or LV1 Classic. LV1 creates scenes and recalls normal console state. |
| App scene configuration | Advanced Show Control's fade overlay for one LV1 scene. It stores duration, targets, parameter scope, and channel scope. |
| Linked scene | An app scene configuration with an available LV1 scene link. Its LV1 scene number and name must match exactly after recall before a fade can start. |
| Unlinked scene | An app scene configuration without an available LV1 scene link. It retains duration and scope, but **Store** and **Recall** are unavailable until you link it. |
| Scope | The channel and parameter-family permission that determines which stored targets Advanced Show Control may apply. |
| Target | A stored value for a scoped parameter. A validated fade moves from the current live value to this value. |
| Cut | An **X-Fade** value of `0`. Scoped targets apply without a timed transition. |
| Fade | The timed movement of eligible fader or pan-family targets after a validated scene recall. |
| Cue list | An ordered, app-managed set of references to app scene configurations. It does not change LV1 scenes. |
| Selected cue | The cue-list entry selected for preparation. Selecting it alone does not make it the active cue. |
| Cued entry | The cue-list entry assigned to **GO**. The status bar identifies its scene when the reference is valid. |
| GO | The action that requests recall of the current valid cued entry. It is unavailable when required cue state is unavailable or a request is pending. |
| SAFE | The application lockout. When active, it blocks application-initiated recalls but does not disable LV1 controls. |
| Session | The `.ascs` document that stores app scene configurations and cue lists. It does not replace an LV1 show file. |
| `.ascs` | The filename extension used by Advanced Show Control session files. |
