package com.rustdroids.dualspace;

import android.app.admin.DevicePolicyManager;
import android.content.ComponentName;
import android.content.Context;
import android.content.pm.ApplicationInfo;
import android.content.pm.PackageManager;
import android.os.UserHandle;
import android.os.UserManager;

import java.util.ArrayList;
import java.util.Collections;
import java.util.List;

/** Everything that talks to Android about the work profile ("dual space"). */
final class Space {
    /** The action the main copy of the app uses to open the copy inside the work profile. */
    static final String ACTION_OPEN = "com.rustdroids.dualspace.OPEN";

    /** Needed to sign in to Google accounts and use the Play Store inside the dual space. */
    static final String[] GOOGLE_PACKAGES = {
            "com.google.android.gms",
            "com.google.android.gsf",
            "com.android.vending",
    };

    private Space() {}

    /** One app in a list. */
    static final class App {
        String pkg;
        String label;
        ApplicationInfo info;
    }

    static DevicePolicyManager dpm(Context c) {
        return (DevicePolicyManager) c.getSystemService(Context.DEVICE_POLICY_SERVICE);
    }

    static ComponentName admin(Context c) {
        return new ComponentName(c, AdminReceiver.class);
    }

    /** True when this copy of the app is the one running INSIDE the work profile. */
    static boolean inProfile(Context c) {
        return dpm(c).isProfileOwnerApp(c.getPackageName());
    }

    /** True when the phone has a work profile next to the main profile. */
    static boolean hasWorkProfile(Context c) {
        UserManager um = (UserManager) c.getSystemService(Context.USER_SERVICE);
        List<UserHandle> profiles = um.getUserProfiles();
        return profiles != null && profiles.size() > 1;
    }

    /**
     * Lists user-installed apps (not system apps).
     * installedHere = true: the apps that live in this work profile.
     * installedHere = false: apps from the main profile that can still be copied in.
     */
    static List<App> list(Context c, boolean installedHere) {
        PackageManager pm = c.getPackageManager();
        List<ApplicationInfo> all = pm.getInstalledApplications(PackageManager.MATCH_UNINSTALLED_PACKAGES);
        List<App> out = new ArrayList<>();
        for (ApplicationInfo ai : all) {
            if (ai.packageName.equals(c.getPackageName())) continue;
            boolean here = (ai.flags & ApplicationInfo.FLAG_INSTALLED) != 0;
            boolean system = (ai.flags & ApplicationInfo.FLAG_SYSTEM) != 0
                    && (ai.flags & ApplicationInfo.FLAG_UPDATED_SYSTEM_APP) == 0;
            if (here != installedHere || system) continue;
            if (installedHere && pm.getLaunchIntentForPackage(ai.packageName) == null) continue;
            App a = new App();
            a.pkg = ai.packageName;
            a.info = ai;
            a.label = String.valueOf(pm.getApplicationLabel(ai));
            out.add(a);
        }
        Collections.sort(out, (x, y) -> x.label.compareToIgnoreCase(y.label));
        return out;
    }

    /** Copies an app that is installed in the main profile into this work profile. */
    static boolean cloneApp(Context c, String pkg) {
        try {
            return dpm(c).installExistingPackage(admin(c), pkg);
        } catch (RuntimeException e) {
            return false;
        }
    }

    /** Turns on Google Play services and the Play Store inside the dual space. Returns how many worked. */
    static int enableGoogle(Context c) {
        int n = 0;
        for (String p : GOOGLE_PACKAGES) {
            if (cloneApp(c, p)) n++;
        }
        return n;
    }
}
