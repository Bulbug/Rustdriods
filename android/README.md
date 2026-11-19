# Dual Space (Android app)

Run a **second copy of your apps** with separate accounts and data, using Android's built-in **work profile**.
It does not hack the system: Android itself keeps the two spaces apart. Copied apps show a small briefcase badge and
live in your launcher's **Work** tab.

## How it works
1. In your normal space, open **Dual Space** and press **Create dual space**. Android creates a work profile.
2. A second copy of Dual Space opens inside it. There you can **copy in** any app that is installed on your phone, and open or remove it.
3. For Google sign-in (Play Store, Gmail), press **Turn on Google apps** once, then sign in from the Work tab.

## Install
1. Download `rustdroids-dual-space-beta.apk` from this repo's **Releases** page (tag `dual-space-beta`).
2. Open it on your phone. Android asks to allow "Install unknown apps" for your browser or file manager. Allow it.
3. Play Protect may warn about an unknown app. This is a hobby app that is not on the Play Store.

Needs **Android 9 or newer**.

## Limits (please read)
- It is **not a virtual phone**. It makes a second *profile*, so you can't run a different Android version inside it.
- Apps must already be installed on your phone; you copy them in.
- Banks, many games, and some messengers refuse to run in a work profile or twice. Multi-accounting in online games can get you banned.
- Some phone makers disable or change work profiles, and a phone managed by a school or company may not allow a second one.
- **Do not uninstall Dual Space while you use the dual space.** That deletes the dual space and everything inside it.
- To remove the dual space: Settings > Accounts (or Passwords & accounts) > Work profile > Remove.

## Build
GitHub builds it for you: every push that changes `android/` runs the **Android** workflow and publishes the APK as the pre-release `dual-space-beta`.
To build locally you need Android Studio (or JDK 17 + the Android SDK + Gradle 8.9): run `gradle assembleRelease` inside this folder.

The signing key `dualspace.jks` is in the repo on purpose, so every build can update the one before it.
That is fine for a hobby app you install yourself. It is **not** suitable for publishing on the Play Store.
