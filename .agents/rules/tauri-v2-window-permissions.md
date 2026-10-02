# Tauri v2 Window Permissions
Whenever you implement custom titlebars, window dragging (`data-tauri-drag-region`), or use the `@tauri-apps/api/window` APIs (like close, minimize, maximize) in a Tauri v2 application:

You MUST ensure that the appropriate window permissions are granted in `src-tauri/capabilities/default.json`. 

If you omit `"window:default"` (or granular permissions like `"window:allow-close"`) from the `permissions` array in the capabilities file, all window interactions will silently fail at runtime.
