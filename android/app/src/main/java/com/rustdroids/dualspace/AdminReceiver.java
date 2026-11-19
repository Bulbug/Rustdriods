package com.rustdroids.dualspace;

import android.app.admin.DeviceAdminReceiver;
import android.app.admin.DevicePolicyManager;
import android.content.ComponentName;
import android.content.Context;
import android.content.Intent;
import android.content.IntentFilter;
import android.content.pm.PackageManager;

/** Runs inside the new work profile when Android has finished creating it. */
public class AdminReceiver extends DeviceAdminReceiver {
    @Override
    public void onProfileProvisioningComplete(Context context, Intent intent) {
        DevicePolicyManager dpm = Space.dpm(context);
        ComponentName admin = Space.admin(context);
        try {
            dpm.setProfileName(admin, "Dual Space");

            // Let the main copy of the app open this copy.
            IntentFilter filter = new IntentFilter(Space.ACTION_OPEN);
            filter.addCategory(Intent.CATEGORY_DEFAULT);
            dpm.addCrossProfileIntentFilter(admin, filter, DevicePolicyManager.FLAG_PARENT_CAN_ACCESS_MANAGED);
            context.getPackageManager().setComponentEnabledSetting(
                    new ComponentName(context, OpenActivity.class),
                    PackageManager.COMPONENT_ENABLED_STATE_ENABLED,
                    PackageManager.DONT_KILL_APP);

            // Google sign-in and the Play Store.
            Space.enableGoogle(context);
        } catch (RuntimeException ignored) {
            // The profile still works; the user can use the buttons inside the app.
        }
        dpm.setProfileEnabled(admin);

        Intent open = new Intent(context, MainActivity.class).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK);
        context.startActivity(open);
    }
}
