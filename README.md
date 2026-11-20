# HoverOver

HoverOver 1.0.0 is a small Windows desktop overlay for placing images above other windows. Its original example use is keeping an “Unregistered HyperCam 2” style label visible while sharing a screen.

## Use

- **Editing mode:** add, select, drag, resize, rotate, flip, change opacity, set X/Y position, and arrange image layers.
- **Display mode:** show the images on a click-through overlay.
- Press **Ctrl+Shift+F1** to switch modes. Starting a second copy opens the editor in the existing instance.
- Right-click an image for delete, flip, layer arrangement, and properties options. Press Enter to deselect the selected image.
- The Properties panel contains center-relative X/Y, opacity, and Z. Screen center is `(0, 0)` and positive Y points down. Larger Z values draw in front.
- Presets are self-contained JSON files that embed their images. Load and Save use Windows file dialogs. Version 1 presets remain supported; new files use preset format version 2.

## Local data

HoverOver stores imported images and its default preset folder in `%APPDATA%\HoverOver`.

**Preset → Clear All Local Data...** counts the files and folders first, then asks for explicit confirmation showing the exact target path. It removes only the verified `%APPDATA%\HoverOver` directory. It does not uninstall HoverOver, delete external preset files, or close the application. Images already loaded in memory remain available; saving afterward can embed those in-memory images even though their original local files were cleared.

For safety, the clear operation refuses to proceed if it cannot enumerate the full directory or finds a symbolic link, junction, or other reparse point in the target tree. If deletion fails partway through, HoverOver reports that the operation may be incomplete. The application creates its local folders again when a later operation needs them.

## Build

Build on Windows with the Rust toolchain:

```powershell
cargo build --release
```

The Windows release profile uses the static Microsoft C runtime configured by `.cargo/config.toml`.
