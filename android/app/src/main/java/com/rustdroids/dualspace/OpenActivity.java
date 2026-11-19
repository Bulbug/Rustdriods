package com.rustdroids.dualspace;

import android.app.Activity;
import android.content.Intent;
import android.os.Bundle;

/** Switched on only inside the work profile: opens the dual space screen when the main copy asks. */
public class OpenActivity extends Activity {
    @Override
    protected void onCreate(Bundle savedInstanceState) {
        super.onCreate(savedInstanceState);
        startActivity(new Intent(this, MainActivity.class));
        finish();
    }
}
