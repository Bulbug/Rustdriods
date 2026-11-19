package com.rustdroids.dualspace;

import android.app.Activity;
import android.os.Bundle;

/** Android asks the app to check its policies at the end of setup. There is nothing to check. */
public class ComplianceActivity extends Activity {
    @Override
    protected void onCreate(Bundle savedInstanceState) {
        super.onCreate(savedInstanceState);
        setResult(RESULT_OK);
        finish();
    }
}
