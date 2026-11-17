# Changelog

## 0.10.2
- New app logo (exe icon, window and taskbar icon, sidebar, README)

## 0.10.1
- Automatic beta releases: every push to main publishes the `beta` pre-release with the exe, a zip, and checksums
- The sidebar shows `beta` and the commit for beta builds

## 0.10.0
- "Get started" button (Setup page, and on an empty Devices page): sets up Java and the Android tools, installs Android 14,
  creates a phone with the preset suggested for your PC, and starts it, in one go
- Safe to repeat: every step is skipped when it is already done, so after a failure it continues where it stopped

## 0.9.1
- ADB Tools redrawn as a stack of panels: a dashed drop zone for APKs, screenshot, installed apps, speed tweaks, shell commands (with example chips), and file copy
- Keymap redrawn: a phone on the left with key markers, profile and keycap tiles on the right, clear status chip in the header

## 0.9.0
- New look: a sidebar with icons, rounded panels, status chips, and one accent colour
- Light, Dark and Match Windows themes; the choice is remembered
- Setup shows "N of 5 ready" with a segmented progress bar; Devices opens with Running / Stopped / Saved state numbers
- New fonts: Plus Jakarta Sans (SIL Open Font License, see assets/fonts/OFL.txt)
- Bigger default window (1000x680)

## 0.8.0
- ADB Tools: "Make the running device faster" (animations off, finish background optimization, free caches,
  turn off optional Google apps, skip app scanning), with Undo for the reversible ones
- Setup tab shows your graphics card and warns when no graphics driver is installed
- The suggested graphics mode (Host GPU for NVIDIA/AMD, Auto for Intel) is pre-selected in the launch options

## 0.7.1
- GitHub Actions: every build now keeps the exe as a downloadable artifact (CI and Release)
- CI builds before it runs the tests, so a failing test no longer blocks the exe
- Release can also be started by hand ("Run workflow"), without making a tag

## 0.7.0
- Keymap tab (experimental): map keyboard keys to taps on the game screen
  - capture the screen, click a spot, press a key; one profile per game, saved as a small text file
  - works only while the Android Emulator window is in front; F10 pauses and resumes
  - taps use `adb input tap` (about 0.1-0.3 s delay, one spot at a time)

## 0.6.0
- ADB Tools: run `adb shell` commands (with example buttons), copy files to and from the emulator, drag any file onto the window to copy it
- The Devices tab suggests a preset from your PC's RAM and CPU
- Edit an existing device's RAM and CPU cores
- See how much disk space each device's saved state uses, and clear it

## 0.5.0
- Packaging: app icon (exe, window, taskbar), version info, MIT license
- Automatic Windows builds and GitHub releases (GitHub Actions)
- Setup tab shows the version and an "Open data folder" button
- New README, troubleshooting guide, and release checklist

## 0.4.0
- ADB Tools tab: drag-and-drop APK install, screenshots, app list and uninstall

## 0.3.0
- Launcher: Start/Stop one device, live log, lean launch options (on by default)

## 0.2.0
- Rewritten in Rust with egui (previously Java + JavaFX)
- Setup, System Images, and Devices tabs; new Lite preset; AOSP images
- Downloads a small Java runtime when the PC has no Java 17+
