// Embeds the icon and version info into the Windows .exe.
// If it fails (for example no resource compiler), the build still succeeds, just without the icon.
fn main() {
    #[cfg(windows)]
    {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("assets/icon.ico");
        res.set("ProductName", "Android Emulator Manager");
        res.set("FileDescription", "Lightweight control panel for the Android Emulator");
        if let Err(e) = res.compile() {
            println!("cargo:warning=could not embed the icon: {e}");
        }
    }
}
