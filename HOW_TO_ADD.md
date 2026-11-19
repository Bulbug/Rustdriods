# Adding the Android app to your repo (Rustdriods)

Copy these into the **root of your repo** (the folder that has `Cargo.toml`), keeping the folder structure:

- `android/`  (the whole Android project; new)
- `.github/workflows/android.yml`  (new: builds the APK)
- `.github/workflows/beta.yml`  (replace the old one: it now ignores changes that only touch `android/`)

Then:
```
git add -A
git commit -m "Add the Dual Space Android app"
git push
```
The **Android** workflow builds the APK (about 3-6 minutes the first time). Look for the pre-release **dual-space-beta** on your Releases page,
or the `dual-space-apk` file under Artifacts on the run page.

If the build fails: open the failed run, open the red step, and send me the last 20 lines.
