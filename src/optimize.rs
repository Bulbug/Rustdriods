//! "Make the running device faster": a few safe, undoable tweaks sent to the emulator with adb.
//! Nothing here touches your PC.
use crate::runner::{self, Log};
use crate::{paths, Res};

/// Google apps that most people never use in an emulator. Turning them off stops their background work.
/// ("Turned off" = disabled for the user; they can be switched back on with Undo.)
pub const OPTIONAL_GOOGLE_APPS: [&str; 9] = [
    "com.google.android.apps.maps",
    "com.google.android.youtube",
    "com.google.android.apps.photos",
    "com.google.android.gm",
    "com.google.android.apps.docs",
    "com.google.android.apps.tachyon",
    "com.google.android.apps.youtube.music",
    "com.google.android.videos",
    "com.google.android.music",
];

#[derive(Clone)]
pub struct OptimizeOptions {
    pub animations_off: bool,
    pub finish_dexopt: bool,
    pub free_caches: bool,
    pub disable_google_apps: bool,
    pub skip_app_scan: bool,
}

impl Default for OptimizeOptions {
    fn default() -> Self {
        // The risk-free ones are on; the ones that change behavior are opt-in.
        OptimizeOptions {
            animations_off: true,
            finish_dexopt: true,
            free_caches: true,
            disable_google_apps: false,
            skip_app_scan: false,
        }
    }
}

/// One adb shell command with a human-readable label.
#[derive(Debug, PartialEq)]
pub struct Step {
    pub label: String,
    pub args: Vec<String>,
}

fn step(label: &str, args: &[&str]) -> Step {
    Step { label: label.to_string(), args: args.iter().map(|s| s.to_string()).collect() }
}

/// Builds the list of commands. `present` = the optional Google apps that exist on the device
/// (when applying) or that are currently turned off (when restoring).
pub fn plan(o: &OptimizeOptions, restore: bool, present: &[String]) -> Vec<Step> {
    let mut v = Vec::new();

    if o.animations_off {
        let value = if restore { "1" } else { "0" };
        let what = if restore { "Turning animations back on" } else { "Turning animations off" };
        for key in ["window_animation_scale", "transition_animation_scale", "animator_duration_scale"] {
            v.push(step(what, &["settings", "put", "global", key, value]));
        }
    }
    if o.skip_app_scan {
        let value = if restore { "1" } else { "0" };
        let what = if restore { "Turning app scanning back on" } else { "Turning app scanning off" };
        for key in ["package_verifier_enable", "verifier_verify_adb_installs"] {
            v.push(step(what, &["settings", "put", "global", key, value]));
        }
    }
    if o.disable_google_apps {
        for pkg in present {
            if restore {
                v.push(step(&format!("Turning {pkg} back on"), &["pm", "enable", pkg]));
            } else {
                v.push(step(&format!("Turning {pkg} off"), &["pm", "disable-user", "--user", "0", pkg]));
            }
        }
    }
    // These two only make sense when applying.
    if !restore && o.finish_dexopt {
        v.push(step(
            "Finishing background app optimization (this can take a few minutes)",
            &["cmd", "package", "bg-dexopt-job"],
        ));
    }
    if !restore && o.free_caches {
        v.push(step("Freeing cached files", &["pm", "trim-caches", "128G"]));
    }
    v
}

/// Runs the steps one by one. A step that fails is reported and skipped, not fatal.
pub fn run(steps: &[Step], log: &Log) -> Res<()> {
    if steps.is_empty() {
        log("Nothing to do. Tick at least one option.".to_string());
        return Ok(());
    }
    for (i, s) in steps.iter().enumerate() {
        log(format!("({}/{}) {} ...", i + 1, steps.len(), s.label));
        let mut args = vec!["-e".to_string(), "shell".to_string()];
        args.extend(s.args.iter().cloned());
        let code = runner::run(paths::adb(), &args, &[], None, log)?;
        if code != 0 {
            log(format!("   skipped (exit code {code})"));
        }
    }
    log("Done.".to_string());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_plan_is_the_safe_set() {
        let p = plan(&OptimizeOptions::default(), false, &[]);
        assert_eq!(p.len(), 5); // 3 animation settings + dexopt + trim caches
        assert_eq!(p[0].args, ["settings", "put", "global", "window_animation_scale", "0"]);
        assert_eq!(p[3].args, ["cmd", "package", "bg-dexopt-job"]);
    }

    #[test]
    fn restore_puts_things_back_and_skips_one_way_steps() {
        let o = OptimizeOptions { disable_google_apps: true, skip_app_scan: true, ..Default::default() };
        let present = vec!["com.google.android.youtube".to_string()];
        let p = plan(&o, true, &present);
        assert!(p.iter().all(|s| s.args[0] != "cmd" && s.args[1] != "trim-caches"));
        assert!(p.iter().any(|s| s.args == ["pm", "enable", "com.google.android.youtube"]));
        assert!(p.iter().any(|s| s.args == ["settings", "put", "global", "package_verifier_enable", "1"]));
        assert!(p.iter().any(|s| s.args.last().map(String::as_str) == Some("1") && s.args[3] == "animator_duration_scale"));
    }

    #[test]
    fn disabling_apps_only_targets_present_ones() {
        let o = OptimizeOptions {
            animations_off: false, finish_dexopt: false, free_caches: false,
            disable_google_apps: true, skip_app_scan: false,
        };
        assert!(plan(&o, false, &[]).is_empty());
        let p = plan(&o, false, &["com.google.android.gm".to_string()]);
        assert_eq!(p.len(), 1);
        assert_eq!(p[0].args, ["pm", "disable-user", "--user", "0", "com.google.android.gm"]);
    }
}
