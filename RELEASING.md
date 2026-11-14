# Putting this on GitHub and making a release

## One time: publish the repo
1. Create an empty repository on GitHub (no README, no license, they already exist here).
2. In this folder:
```
git init
git add .
git commit -m "Android Emulator Manager 0.5.0"
git branch -M main
git remote add origin https://github.com/<your-name>/android-emulator-manager.git
git push -u origin main
```
3. On GitHub, open the **Actions** tab. The `CI` workflow runs the tests and a release build on Windows.
4. Check that `Cargo.lock` was committed (the release workflow needs it).
5. Add 2-3 screenshots to `docs/screenshots/` and show them in `README.md`.

## Before every release
- [ ] Test on real Windows with the real emulator: Setup, install an image, create a device (Lite), start, install an APK, screenshot, stop.
- [ ] The two download links still work: `CMDLINE_TOOLS_URL` in `src/sdk.rs` and `JRE_URL` in `src/java.rs`.
- [ ] Update the version in `Cargo.toml` and add an entry in `CHANGELOG.md`.
- [ ] `cargo test` passes and `cargo build --release` builds with no warnings.

## Make the release
```
git add -A
git commit -m "Release 0.5.0"
git tag v0.5.0
git push origin main --tags
```
Pushing a tag that starts with `v` runs `.github/workflows/release.yml`. It builds on Windows and creates a GitHub
release with the `.exe`, a `.zip`, and `SHA256SUMS.txt`.

## After the release
- [ ] Download the `.exe` from the release page and run it on a clean Windows PC or user account.
- [ ] Edit the release notes (they are auto-generated) and add the SmartScreen note.
- [ ] If something is wrong, delete the release and tag (`git push --delete origin v0.5.0`), fix it, and tag again.
