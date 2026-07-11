# Terminology

| Term | Definition |
| --- | --- |
| LV1 scene | A scene created and owned by Waves eMotion LV1 or LV1 Classic. LV1 remains the authority for scene creation and normal recall. |
| App scene configuration | The application's stored fade overlay for one LV1 scene. It contains fade duration, stored targets, and parameter and channel scope. |
| Linked scene | An app scene configuration whose stored LV1 scene number and name identify an available LV1 scene. It can be stored and recalled after validation. |
| Unlinked scene | An app scene configuration without an available LV1 scene link. It retains its stored fade data, but Store and Recall are unavailable until it is linked. |
| Scope | The channels and parameter families the application is permitted to apply for a scene configuration. |
| Target | A value stored for a scoped parameter. A validated fade moves from the current live value to this stored value. |
| Cut | A fade duration of `0`, which applies the scoped target without a timed transition. |
| Fade | The timed application of scoped fader or pan-family targets after a validated scene recall. |
| Cue list | An ordered, application-managed list of references to app scene configurations. It does not create or modify LV1 scenes. |
| Selected cue | The cue-list entry currently selected for preparation. Selecting it alone does not make it the active cue. |
| Cued entry | The cue-list entry currently assigned for GO. The status bar identifies its referenced scene when that scene is valid. |
| GO | The action that requests recall of the current valid cued entry. It is unavailable when the required cue state is unavailable or a request is pending. |
| SAFE | The application lockout control. When active, it blocks application-initiated recalls but does not disable LV1 controls. |
| Session | The application's saved document containing app-managed scene configurations and cue lists. It does not replace an LV1 show file. |
| `.ascs` | The filename extension used by Advanced Show Control session files. |
