# Tauri v2 Window & Webview Permissions

Whenever you implement custom titlebars, window dragging (`data-tauri-drag-region`), or use the `@tauri-apps/api/window` APIs (like close, minimize, maximize) in a Tauri v2 application:

1. You MUST explicitly grant granular permissions in `src-tauri/capabilities/default.json`.
2. `"core:window:default"` ONLY provides read-only getters (e.g. `is-minimized`, `inner-size`). It does NOT grant permission to close, minimize, maximize, or drag!
3. To enable window controls and dragging, you MUST explicitly include:
   - `"core:window:default"`
   - `"core:window:allow-start-dragging"`
   - `"core:window:allow-close"`
   - `"core:window:allow-minimize"`
   - `"core:window:allow-maximize"`
   - `"core:window:allow-unmaximize"`
   - `"core:window:allow-toggle-maximize"`
   - `"core:webview:default"`
4. Omitting any of these will cause Tauri v2 IPC to return 400 Bad Request or silently reject the action.
