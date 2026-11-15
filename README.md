# Android Emulator Manager

A tiny Windows control panel for Google's official Android Emulator.
It downloads the SDK tools for you, manages system images and virtual devices,
launches them with lean settings, and installs apps by drag and drop.

<img src="assets/icon.png" width="96" alt="icon">

## Why it's light
- One small native `.exe`. No installer, no Java app, no browser engine.
- Written in Rust with [egui](https://github.com/emilk/egui). It idles at ~0% CPU and does nothing in the background.
- A **Lite** device preset (1.5 GB RAM, 2 cores) and lean launch flags (muted audio, no boot animation) for low-end PCs.
- Nothing is bundled from Google: the SDK tools are downloaded on demand into one folder you can delete.

## Features
- **Setup:** checks virtualization, then downloads the Android SDK tools (and a small Java runtime if your PC has no Java 17+).
- **System Images:** pick from 11 recommended Android 11-15 images (Google APIs, Google Play, or the lightest AOSP).
- **Devices:** create virtual devices from presets (Lite, Low-end PC, Balanced, Gaming). The app suggests one for your PC. Start and stop devices, tweak launch options, change RAM and CPU later, and clear saved state to free disk space.
- **Keymap (experimental):** play with the keyboard. Capture the screen, click where a button is, press the key to bind. One profile per game. See the notes below.
- **ADB Tools:** drag an `.apk` onto the window to install it, take screenshots, list and uninstall apps, run `adb shell` commands, and copy files to and from the emulator.

<!-- TODO: add screenshots here, e.g. docs/screenshots/devices.png -->

## Download
Get `android-emulator-manager-<version>-windows-x64.zip` (or the plain `.exe`) from the
[Releases](../../releases) page. No install needed: just run it.
Each release also lists SHA-256 checksums (`SHA256SUMS.txt`) so you can verify the download.

> **Windows SmartScreen:** the exe is not code-signed, so Windows may say "Windows protected your PC".
> Click **More info > Run anyway**. Some antivirus programs also flag new, unsigned programs by mistake.
> You can always build it yourself from source (see below).

## Requirements
- Windows 10 or 11, 64-bit
- CPU virtualization turned on in the BIOS, plus the **Windows Hypervisor Platform** Windows feature
- About 5-10 GB of free disk space (SDK tools + one image + one device)
- Internet for the first setup

## Quick start
1. Open the app and go to **Setup**. Press **Set up Android SDK** and accept the license prompt.
2. In **System Images**, install one image (the Google APIs image of Android 14 is a good start; AOSP is the lightest).
3. In **Devices**, type a name, choose the image and a preset (try **Lite**), and press **Create device**.
4. Select the device and press **Start selected**. The first boot is slow; later boots are quick.
5. In **ADB Tools**, press Refresh once the emulator has booted, then drag an `.apk` onto the window.

## Keymap notes (experimental)
1. Start a device and open your game. In the **Keymap** tab, create a profile and press **Capture screen**.
2. Click a spot on the picture, then press the key you want for it. Repeat for each button.
3. Press **Turn mapping ON**, click into the emulator window, and play. **F10** pauses and resumes (handy for typing).
- Taps use `adb input tap`, so they arrive about 0.1-0.3 s late and only one spot can be tapped at a time (no joystick or multi-touch). That is fine for casual, turn-based, and tower-defense games, not for fast action games.
- Mapping only works while the **Android Emulator window is in front**. Mapped keys are blocked from reaching the emulator, and every other key behaves normally.
- Some games detect or ignore injected input.
- Profiles are small text files in `%LOCALAPPDATA%\AndroidEmulatorManager\keymaps`.

## Where things are stored
Everything lives in `%LOCALAPPDATA%\AndroidEmulatorManager` (the Setup tab has an "Open data folder" button):
`sdk\` (SDK tools, images, emulator), `avd\` (your devices), `jre\` (only if Java was downloaded).
Screenshots go to `Pictures\AndroidEmulatorManager`.
To uninstall completely, delete that folder and the `.exe`.

## Troubleshooting
| Problem | What to try |
|---|---|
| Virtualization says "Missing" | Turn on Intel VT-x / AMD SVM in the BIOS, and enable **Windows Hypervisor Platform** in "Turn Windows features on or off", then restart. |
| Download fails with HTTP 404 | Google moved the file. Put the current "Command line tools only" Windows link into `CMDLINE_TOOLS_URL` in `src/sdk.rs` and rebuild. |
| The window does not open | The app uses OpenGL. Update your graphics driver (virtual machines need 3D acceleration). |
| Emulator closes right away or looks broken | Read the log panel. In **Launch options**, try Graphics: *ANGLE* or *Software*. |
| An app fails with `NO_MATCHING_ABIS` | That app has no build for the emulator's x86_64 CPU. Try an Android 11+ image or a different APK. |
| ADB Tools says "No running emulator found" | Wait until the device has fully booted, then press Refresh. |

## Build from source
Install Rust from <https://rustup.rs>. On Windows, also install the *Build Tools for Visual Studio* (C++ workload).
```
cargo run --release
```
The exe is created at `target\release\android-emulator-manager.exe`.
Run the tests with `cargo test`.

## Project layout
```
src/main.rs        window setup
src/app.rs         all screens (Setup, System Images, Devices, ADB Tools)
src/sdk.rs         downloads and installs the SDK tools
src/java.rs        finds Java 17+ or downloads a small runtime
src/images.rs      the recommended system image list
src/devices.rs     virtual devices and presets
src/launcher.rs    starting and stopping the emulator
src/adb.rs         install apps, screenshots, app list
src/runner.rs      runs programs and streams their output
```

## Roadmap
- [x] Phase 1: environment check + SDK setup
- [x] Phase 2: system image manager
- [x] Phase 3: virtual device manager (presets)
- [x] Phase 4: launcher (lean launch flags)
- [x] Phase 5: ADB tools
- [x] Phase 6a: packaging (icon, CI, releases)
- [x] ADB shell and file copy
- [x] Smarter performance profiles (suggested preset, edit device, saved-state cleanup)
- [x] Keymapping for games (v1: `adb input tap`)
- [ ] Faster touch injection (multi-touch, joystick) if v1 feels too slow

## Legal
This project is not affiliated with or endorsed by Google. Android is a trademark of Google LLC.
The app downloads Google's Android SDK tools, emulator, and system images directly from Google when you press
*Set up*, and you accept Google's [SDK license](https://developer.android.com/studio/terms) at that moment.
If Java is missing, a runtime from [Eclipse Temurin](https://adoptium.net) is downloaded.

## License
[MIT](LICENSE)
