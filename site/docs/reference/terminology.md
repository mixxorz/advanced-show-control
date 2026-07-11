# Terminology

| Term | Meaning |
| --- | --- |
| LV1 scene | A console scene created and recalled in LV1. |
| Scene fade setting | The Advanced Show Control setting linked to one LV1 scene. It stores fade time, values, and scope. |
| Linked setting | A scene fade setting connected to an available LV1 scene. You can store and recall it. |
| Unlinked setting | A scene fade setting without an LV1 scene. It keeps its fade time and scope, but **Store** and **Recall** are unavailable. |
| Scope | Selects which channels and which controls, **FADER** and **PAN**, can move during the fade. |
| Target | The value a scoped control moves to during the fade. |
| Cut | An **X-Fade** value of `0`; scoped controls change without a timed move. |
| Fade | The timed move from the controls' positions at recall to their stored values. |
| Cue list | An ordered list of scene fade settings. |
| Cued entry | The cue-list entry prepared for **GO**. |
| GO | Recalls the cued scene fade setting. |
| SAFE | Blocks recalls started by Advanced Show Control without disabling LV1 controls. |
| Session | An `.ascs` file containing scene fade settings and cue lists. |
