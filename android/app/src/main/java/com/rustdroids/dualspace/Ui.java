package com.rustdroids.dualspace;

import android.content.Context;
import android.content.res.Configuration;
import android.graphics.Typeface;
import android.graphics.drawable.GradientDrawable;
import android.view.Gravity;
import android.view.View;
import android.widget.Button;
import android.widget.LinearLayout;
import android.widget.TextView;

/** Small helpers for building the screens in code: colours, rounded panels, buttons. Light and dark follow the phone. */
final class Ui {
    final Context c;
    final boolean dark;
    final int bg, card, line, text, muted, accent, onAccent, tint, tintText;

    Ui(Context c) {
        this.c = c;
        dark = (c.getResources().getConfiguration().uiMode & Configuration.UI_MODE_NIGHT_MASK)
                == Configuration.UI_MODE_NIGHT_YES;
        bg = dark ? 0xFF0D0F1C : 0xFFF7F8FC;
        card = dark ? 0xFF161A31 : 0xFFFFFFFF;
        line = dark ? 0xFF252A49 : 0xFFE8E9F2;
        text = dark ? 0xFFECEEFB : 0xFF14172B;
        muted = dark ? 0xFFA5ABCF : 0xFF5E6482;
        accent = dark ? 0xFF8B8BFF : 0xFF5B5BF0;
        onAccent = dark ? 0xFF0D0F1C : 0xFFFFFFFF;
        tint = dark ? 0xFF232857 : 0xFFEEEEFF;
        tintText = dark ? 0xFFC3C4FF : 0xFF3535C4;
    }

    int dp(float v) {
        return Math.round(v * c.getResources().getDisplayMetrics().density);
    }

    GradientDrawable shape(int fill, int stroke, float radiusDp) {
        GradientDrawable g = new GradientDrawable();
        g.setColor(fill);
        g.setCornerRadius(dp(radiusDp));
        if (stroke != 0) g.setStroke(Math.max(1, dp(1)), stroke);
        return g;
    }

    TextView label(String s, int sp, boolean bold, int color) {
        TextView t = new TextView(c);
        t.setText(s);
        t.setTextSize(sp);
        t.setTextColor(color);
        if (bold) t.setTypeface(Typeface.DEFAULT_BOLD);
        return t;
    }

    LinearLayout vbox() {
        LinearLayout l = new LinearLayout(c);
        l.setOrientation(LinearLayout.VERTICAL);
        return l;
    }

    LinearLayout hbox() {
        LinearLayout l = new LinearLayout(c);
        l.setOrientation(LinearLayout.HORIZONTAL);
        l.setGravity(Gravity.CENTER_VERTICAL);
        return l;
    }

    /** A rounded white (or dark) panel. */
    LinearLayout card() {
        LinearLayout l = vbox();
        l.setBackground(shape(card, line, 18));
        l.setPadding(dp(18), dp(16), dp(18), dp(16));
        LinearLayout.LayoutParams lp = new LinearLayout.LayoutParams(
                LinearLayout.LayoutParams.MATCH_PARENT, LinearLayout.LayoutParams.WRAP_CONTENT);
        lp.bottomMargin = dp(14);
        l.setLayoutParams(lp);
        return l;
    }

    Button button(String s, boolean primary, View.OnClickListener click) {
        Button b = new Button(c);
        b.setText(s);
        b.setAllCaps(false);
        b.setTextSize(14);
        b.setTypeface(Typeface.DEFAULT_BOLD);
        b.setTextColor(primary ? onAccent : text);
        b.setBackground(shape(primary ? accent : 0x00000000, primary ? 0 : line, 12));
        b.setMinHeight(dp(46));
        b.setMinimumHeight(dp(46));
        b.setStateListAnimator(null);
        b.setPadding(dp(18), 0, dp(18), 0);
        b.setOnClickListener(click);
        return b;
    }

    View gap(float dpv) {
        View v = new View(c);
        v.setLayoutParams(new LinearLayout.LayoutParams(1, dp(dpv)));
        return v;
    }
}
