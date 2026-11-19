package com.rustdroids.dualspace;

import android.app.Activity;
import android.app.AlertDialog;
import android.app.admin.DevicePolicyManager;
import android.content.ActivityNotFoundException;
import android.content.ComponentName;
import android.content.Intent;
import android.content.pm.PackageManager;
import android.net.Uri;
import android.os.Bundle;
import android.text.Editable;
import android.text.TextWatcher;
import android.view.Gravity;
import android.view.View;
import android.widget.EditText;
import android.widget.ImageView;
import android.widget.LinearLayout;
import android.widget.ScrollView;
import android.widget.TextView;
import android.widget.Toast;

import java.util.List;
import java.util.Locale;

/**
 * One screen, two jobs:
 * - in the main profile it sets up the dual space (the work profile) and opens it;
 * - inside the work profile it copies apps in and starts them.
 */
public class MainActivity extends Activity {
    private static final int REQ_SETUP = 1;
    private static final int MAX_ROWS = 40;

    private Ui ui;
    private boolean inProfile;
    private LinearLayout content;
    private LinearLayout cloneList;
    private LinearLayout clonedList;
    private List<Space.App> cloneable;
    private String filter = "";

    @Override
    protected void onCreate(Bundle savedInstanceState) {
        super.onCreate(savedInstanceState);
        ui = new Ui(this);
        inProfile = Space.inProfile(this);
        if (inProfile) {
            // The entry point used by the main copy of the app exists only inside the work profile.
            getPackageManager().setComponentEnabledSetting(
                    new ComponentName(this, OpenActivity.class),
                    PackageManager.COMPONENT_ENABLED_STATE_ENABLED,
                    PackageManager.DONT_KILL_APP);
        }
        getWindow().setStatusBarColor(ui.bg);
        getWindow().setNavigationBarColor(ui.bg);
        if (!ui.dark) {
            getWindow().getDecorView().setSystemUiVisibility(
                    View.SYSTEM_UI_FLAG_LIGHT_STATUS_BAR | View.SYSTEM_UI_FLAG_LIGHT_NAVIGATION_BAR);
        }
    }

    @Override
    protected void onResume() {
        super.onResume();
        build();
    }

    // ------------------------------------------------------------------ screens

    private void build() {
        ScrollView scroll = new ScrollView(this);
        scroll.setBackgroundColor(ui.bg);
        content = ui.vbox();
        content.setPadding(ui.dp(20), ui.dp(24), ui.dp(20), ui.dp(32));
        scroll.addView(content);
        setContentView(scroll);

        addHeader();
        if (inProfile) {
            buildProfileScreen();
        } else {
            buildMainScreen();
        }
    }

    private void addHeader() {
        LinearLayout row = ui.hbox();
        ImageView icon = new ImageView(this);
        icon.setImageDrawable(getApplicationInfo().loadIcon(getPackageManager()));
        row.addView(icon, new LinearLayout.LayoutParams(ui.dp(44), ui.dp(44)));

        LinearLayout titles = ui.vbox();
        titles.setPadding(ui.dp(12), 0, 0, 0);
        titles.addView(ui.label("Dual Space", 24, true, ui.text));
        titles.addView(ui.label(
                inProfile ? "Inside your dual space" : "A second copy of your apps, with separate accounts",
                13, false, ui.muted));
        row.addView(titles);
        content.addView(row);
        content.addView(ui.gap(18));
    }

    /** The screen in the main profile. */
    private void buildMainScreen() {
        boolean ready = Space.hasWorkProfile(this);

        LinearLayout status = ui.card();
        status.addView(ui.label(ready ? "Your dual space is ready" : "Not set up yet", 18, true, ui.text));
        status.addView(ui.gap(6));
        status.addView(ui.label(ready
                ? "Open it, copy the apps you want a second account for, and they run on their own."
                : "This creates a separate space on your phone. Apps you copy into it keep their own accounts and data. "
                + "Android marks them with a small briefcase badge.", 14, false, ui.muted));
        status.addView(ui.gap(14));
        if (ready) {
            status.addView(ui.button("Open dual space", true, v -> openDualSpace()));
            status.addView(ui.gap(8));
            status.addView(ui.label(
                    "You can also find the copies in your launcher's Work tab.", 13, false, ui.muted));
        } else {
            status.addView(ui.button("Create dual space", true, v -> startSetup()));
        }
        content.addView(status);

        LinearLayout info = ui.card();
        info.addView(ui.label("Good to know", 16, true, ui.text));
        info.addView(ui.gap(8));
        info.addView(ui.label("- An app must be installed on your phone first, then you copy it in.", 14, false, ui.muted));
        info.addView(ui.label("- Some apps (banks, many games) refuse to run twice.", 14, false, ui.muted));
        info.addView(ui.label("- To sign in with Google inside the dual space, turn on the Google apps there.", 14, false, ui.muted));
        info.addView(ui.label("- Do not uninstall Dual Space while you use the dual space: that deletes the dual space and everything in it.", 14, false, ui.muted));
        info.addView(ui.label("- To remove it: Settings > Accounts (or Passwords & accounts) > Work profile > Remove. "
                + "This deletes everything inside.", 14, false, ui.muted));
        content.addView(info);
    }

    /** The screen inside the work profile. */
    private void buildProfileScreen() {
        LinearLayout google = ui.card();
        google.addView(ui.label("Google sign-in", 16, true, ui.text));
        google.addView(ui.gap(6));
        google.addView(ui.label("Turn this on once, then open the Play Store from the Work tab to sign in.", 14, false, ui.muted));
        google.addView(ui.gap(12));
        google.addView(ui.button("Turn on Google apps", false, v -> {
            int n = Space.enableGoogle(this);
            toast(n > 0 ? "Google apps turned on." : "Could not turn on the Google apps on this phone.");
        }));
        content.addView(google);

        LinearLayout cloned = ui.card();
        cloned.addView(ui.label("Apps in your dual space", 16, true, ui.text));
        cloned.addView(ui.gap(8));
        clonedList = ui.vbox();
        clonedList.addView(ui.label("Loading...", 14, false, ui.muted));
        cloned.addView(clonedList);
        content.addView(cloned);

        LinearLayout add = ui.card();
        add.addView(ui.label("Copy an app in", 16, true, ui.text));
        add.addView(ui.gap(8));
        EditText search = new EditText(this);
        search.setHint("Search your apps");
        search.setSingleLine(true);
        search.setTextColor(ui.text);
        search.setHintTextColor(ui.muted);
        search.setBackground(ui.shape(ui.bg, ui.line, 12));
        search.setPadding(ui.dp(14), ui.dp(10), ui.dp(14), ui.dp(10));
        search.addTextChangedListener(new TextWatcher() {
            @Override public void beforeTextChanged(CharSequence s, int a, int b, int c) {}
            @Override public void onTextChanged(CharSequence s, int a, int b, int c) {}
            @Override public void afterTextChanged(Editable s) {
                filter = s.toString().trim().toLowerCase(Locale.ROOT);
                renderCloneable();
            }
        });
        add.addView(search);
        add.addView(ui.gap(10));
        cloneList = ui.vbox();
        cloneList.addView(ui.label("Loading...", 14, false, ui.muted));
        add.addView(cloneList);
        content.addView(add);

        loadLists();
    }

    /** Looks up the apps on a background thread (it can take a moment), then fills the lists. */
    private void loadLists() {
        new Thread(() -> {
            final List<Space.App> here = Space.list(this, true);
            final List<Space.App> there = Space.list(this, false);
            runOnUiThread(() -> {
                if (isFinishing() || isDestroyed()) return;
                cloneable = there;
                renderCloned(here);
                renderCloneable();
            });
        }).start();
    }

    private void renderCloned(List<Space.App> apps) {
        clonedList.removeAllViews();
        if (apps.isEmpty()) {
            clonedList.addView(ui.label("Nothing copied in yet.", 14, false, ui.muted));
            return;
        }
        for (Space.App a : apps) {
            clonedList.addView(appRow(a, "Open", v -> openApp(a.pkg), "Remove", v -> removeApp(a.pkg)));
        }
    }

    private void renderCloneable() {
        if (cloneList == null || cloneable == null) return;
        cloneList.removeAllViews();
        int shown = 0, matches = 0;
        for (Space.App a : cloneable) {
            if (!filter.isEmpty() && !a.label.toLowerCase(Locale.ROOT).contains(filter)) continue;
            matches++;
            if (shown >= MAX_ROWS) continue;
            shown++;
            cloneList.addView(appRow(a, "Copy in", v -> cloneApp(a), null, null));
        }
        if (matches == 0) {
            cloneList.addView(ui.label(cloneable.isEmpty()
                    ? "No other apps found."
                    : "No app matches your search.", 14, false, ui.muted));
        } else if (matches > shown) {
            cloneList.addView(ui.label("Showing " + shown + " of " + matches + ". Type to search.", 13, false, ui.muted));
        }
    }

    private View appRow(Space.App a, String label1, View.OnClickListener c1, String label2, View.OnClickListener c2) {
        LinearLayout row = ui.hbox();
        row.setPadding(0, ui.dp(6), 0, ui.dp(6));

        ImageView icon = new ImageView(this);
        try {
            icon.setImageDrawable(a.info.loadIcon(getPackageManager()));
        } catch (RuntimeException ignored) {
            // an app whose icon can't be read: show no icon
        }
        row.addView(icon, new LinearLayout.LayoutParams(ui.dp(40), ui.dp(40)));

        TextView name = ui.label(a.label, 14, true, ui.text);
        name.setSingleLine(true);
        name.setEllipsize(android.text.TextUtils.TruncateAt.END);
        LinearLayout.LayoutParams nlp = new LinearLayout.LayoutParams(0, LinearLayout.LayoutParams.WRAP_CONTENT, 1f);
        nlp.leftMargin = ui.dp(12);
        nlp.rightMargin = ui.dp(8);
        row.addView(name, nlp);

        row.addView(small(label1, true, c1));
        if (label2 != null) {
            View b2 = small(label2, false, c2);
            ((LinearLayout.LayoutParams) b2.getLayoutParams()).leftMargin = ui.dp(6);
            row.addView(b2);
        }
        return row;
    }

    private View small(String label, boolean primary, View.OnClickListener click) {
        android.widget.Button b = ui.button(label, primary, click);
        b.setMinHeight(ui.dp(38));
        b.setMinimumHeight(ui.dp(38));
        b.setTextSize(13);
        b.setPadding(ui.dp(12), 0, ui.dp(12), 0);
        b.setLayoutParams(new LinearLayout.LayoutParams(LinearLayout.LayoutParams.WRAP_CONTENT, ui.dp(38)));
        return b;
    }

    // ------------------------------------------------------------------ actions

    private void startSetup() {
        DevicePolicyManager dpm = Space.dpm(this);
        if (!dpm.isProvisioningAllowed(DevicePolicyManager.ACTION_PROVISION_MANAGED_PROFILE)) {
            new AlertDialog.Builder(this)
                    .setTitle("Can't create it right now")
                    .setMessage("Your phone does not allow a new dual space. This happens when one already exists, "
                            + "or when your phone is managed by a school or company.")
                    .setPositiveButton("OK", null)
                    .show();
            return;
        }
        Intent i = new Intent(DevicePolicyManager.ACTION_PROVISION_MANAGED_PROFILE);
        i.putExtra(DevicePolicyManager.EXTRA_PROVISIONING_DEVICE_ADMIN_COMPONENT_NAME, Space.admin(this));
        try {
            startActivityForResult(i, REQ_SETUP);
        } catch (ActivityNotFoundException e) {
            toast("This phone does not support a dual space.");
        }
    }

    @Override
    protected void onActivityResult(int requestCode, int resultCode, Intent data) {
        super.onActivityResult(requestCode, resultCode, data);
        if (requestCode != REQ_SETUP) return;
        if (resultCode == RESULT_OK) {
            toast("Creating your dual space. This takes about a minute.");
        } else {
            toast("Setup was cancelled.");
        }
    }

    private void openDualSpace() {
        try {
            startActivity(new Intent(Space.ACTION_OPEN).addCategory(Intent.CATEGORY_DEFAULT));
        } catch (ActivityNotFoundException e) {
            new AlertDialog.Builder(this)
                    .setTitle("Open it from your launcher")
                    .setMessage("Open the Work tab in your app drawer (the apps with a briefcase badge) "
                            + "and start Dual Space from there.")
                    .setPositiveButton("OK", null)
                    .show();
        }
    }

    private void cloneApp(Space.App a) {
        boolean ok = Space.cloneApp(this, a.pkg);
        toast(ok ? a.label + " copied in." : "Could not copy " + a.label + ".");
        if (ok) loadLists();
    }

    private void openApp(String pkg) {
        Intent launch = getPackageManager().getLaunchIntentForPackage(pkg);
        if (launch == null) {
            toast("This app can't be opened.");
            return;
        }
        startActivity(launch);
    }

    private void removeApp(String pkg) {
        startActivity(new Intent(Intent.ACTION_DELETE, Uri.parse("package:" + pkg)));
    }

    private void toast(String msg) {
        Toast.makeText(this, msg, Toast.LENGTH_LONG).show();
    }
}
