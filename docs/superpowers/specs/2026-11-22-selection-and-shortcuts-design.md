# Selection and Shortcuts Redesign

## Goal

Make 2D and 3D canvas editing follow the editing conventions people already know from macOS design tools, while preserving the existing keyboard, trackpad, and DualSense workflows.

## Standard shortcuts

- `Cmd/Ctrl+S`: save.
- `Cmd/Ctrl+Z`: undo; `Cmd/Ctrl+Shift+Z` and `Ctrl+Y`: redo.
- `Cmd/Ctrl+C`, `Cmd/Ctrl+V`, `Cmd/Ctrl+X`: copy, paste, cut.
- `Cmd/Ctrl+D`: duplicate without changing the clipboard.
- `Cmd/Ctrl+A`: select all visible nodes in the active screen.
- `Delete` / `Backspace`: delete the selection.
- `Esc`: clear selection or close the guide.
- Arrow keys nudge; `Shift` plus arrows performs a large nudge.

Shortcuts do not run while a text field, textarea, select, or contenteditable element has focus. Native clipboard behavior remains available inside forms.

## Selection

- Click selects one node.
- `Shift` or `Cmd/Ctrl` click toggles a node in the current selection.
- Dragging from empty canvas draws a marquee.
- `Shift` plus marquee adds the intersected nodes to the selection.
- `Cmd/Ctrl+A` selects all visible nodes.
- The last selected node is the primary node used by the single-node inspector.
- The tree highlights the full selection.
- Undo, deletion, and screen changes remove stale selected IDs.
- Hovering an unselected node or device draws a weak multi-pass glow around its bounds so sparse geometry remains discoverable.

## Clipboard and history

- The application owns a typed clipboard for 2D and 3D nodes.
- Copy works across screens when the destination screen has the same type.
- Paste generates unique IDs, preserves relative geometry, offsets each paste, selects the pasted nodes, and creates one undo entry.
- Cut, group move, group nudge, group delete, and group paste are atomic one-step undo operations.
- Locked nodes are never transformed or deleted by group operations.
- Duplicate does not overwrite the clipboard.

## 2D interaction

- Plain left drag on empty canvas starts marquee selection.
- Dragging a selected node moves the whole unlocked selection.
- Group snapping is calculated from the selection bounds center.
- Selection outlines are drawn for every selected node; corner resize handles remain a single-selection affordance.
- `Q/E` rotation remains a canvas-only shortcut for a single selected node.
- Space or middle-button drag pans; pinch or modified wheel zooms.

## 3D interaction

- Plain left drag on empty canvas starts a screen-space marquee.
- `Option/Alt` plus left drag orbits the camera.
- Dragging a device moves the unlocked selection on the ground plane; dragging from empty space starts a marquee. Dragging an unselected device selects it first.
- Device bounds are projected to screen space for marquee hits.
- Imported GLB child meshes inherit the device picking tag.
- Selection rings are shown for every selected device.
- WASD/QE and D-pad nudges apply to the entire unlocked selection as one undo entry.

## Surface updates

- The inspector shows full properties for one node and a multi-selection summary with bulk actions for multiple nodes.
- The status bar and tree reflect the selection count.
- The operation guide and README document the new shortcuts and marquee gestures.
- Browser acceptance tests cover copy, paste, cut, select-all, marquee selection, group movement, and undo atomicity in 2D and 3D.
