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
- **Faster:** one-click speed tweaks for the running device (animations off, background optimization, optional Google apps off) and a graphics-card check with a suggested graphics mode.
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

**Latest beta:** every change is published automatically on the Releases page under the tag `beta` (a pre-release). It is the newest code and may have bugs.

## Requirements
- Windows 10 or 11, 64-bit
- CPU virtualization turned on in the BIOS, plus the **Windows Hypervisor Platform** Windows feature
- About 5-10 GB of free disk space (SDK tools + one image + one device)
- Internet for the first setup

## Quick start
**Fastest way:** open **Setup** and press **Get started**. It does steps 1-4 below for you.

Or step by step:
1. Open the app and go to **Setup**. Press **Set up Android SDK** and accept the license prompt.
2. In **System Images**, install one image (the Google APIs image of Android 14 is a good start; AOSP is the lightest).
3. In **Devices**, type a name, choose the image and a preset (try **Lite**), and press **Create device**.
4. Select the device and press **Start selected**. The first boot is slow; later boots are quick.
5. In **ADB Tools**, press Refresh once the emulator has booted, then drag an `.apk` onto the window.

## Making it faster
What matters most, in order:
1. **Virtualization on** (Setup tab shows OK) and enough free RAM. Close heavy programs before starting a device.
2. **Graphics:** a real graphics driver installed (the Setup tab warns if not) and the suggested *Graphics* mode in the launch options. *Host GPU* is fastest on NVIDIA/AMD cards.
3. **A lighter device:** the *Lite* preset, an *AOSP* image, and a SSD instead of a hard disk.
4. **ADB Tools > Make the running device faster:** turn off animations and finish the first-boot background optimization (the emulator feels slow for a while after the very first boot until that finishes).

## Keymap notes (experimental)
1. Start a device and open your game. In the **Keymap** tab, create a profile and press **Capture screen**.
2. Click a spot on the picture, then press the key you want for it. Repeat for each button.
3. Press **Turn mapping ON**, click into the emulator window, and play. **F10** pauses and resumes (handy for typing).
- Taps use `adb input tap`, so they arrive about 0.1-0.3 s late and only one spot can be tapped at a time (no joystick or multi-touch). That is fine for casual, turn-based, and tower-defense games, not for fast action games.
- Mapping only works while the **Android Emulator window is in front**. Mapped keys are blocked from reaching the emulator, and every other key behaves normally.
- Some games detect or ignore injected input.
- Profiles are small text files in `%LOCALAPPDATA%\AndroidEmulatorManager\keymaps`.

## If something goes wrong
- Press **Copy log** in the log bar at the bottom and paste it into your bug report. It includes your version, PC details, and what is installed, but nothing personal.
- If the app crashes, it saves a report and shows it in the log the next time you open it.

## Updates and privacy
When the app starts it asks GitHub (`api.github.com`) once whether a newer version exists. Nothing about you or your PC is sent.
Turn it off on the Setup page ("Check for updates when the app starts"). Builds you make yourself don't check at all.

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
src/app.rs         app state and actions
src/app/views.rs   window layout and the Setup, Images and Devices pages
src/theme.rs       colours, fonts, light and dark
src/widgets.rs     buttons, chips, panels, rows
src/icons.rs       the line icons
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

## Themes
The switch at the bottom of the sidebar picks **Light**, **Dark**, or **Match Windows**. Your choice is remembered.

## Credits
The interface font is [Plus Jakarta Sans](https://github.com/tokotype/PlusJakartaSans) by Tokotype, used under the SIL Open Font License 1.1 (`assets/fonts/OFL.txt`).

## Legal
The Android robot in the logo is reproduced or modified from work created and shared by Google and used according to terms described in the Creative Commons 3.0 Attribution License.

This project is not affiliated with or endorsed by Google. Android is a trademark of Google LLC.
The app downloads Google's Android SDK tools, emulator, and system images directly from Google when you press
*Set up*, and you accept Google's [SDK license](https://developer.android.com/studio/terms) at that moment.
If Java is missing, a runtime from [Eclipse Temurin](https://adoptium.net) is downloaded.

## License
[MIT](LICENSE)
