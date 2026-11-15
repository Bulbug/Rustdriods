# Changelog

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
