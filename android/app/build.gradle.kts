plugins {
    id("com.android.application")
}

android {
    namespace = "com.rustdroids.dualspace"
    compileSdk = 34

    defaultConfig {
        applicationId = "com.rustdroids.dualspace"
        minSdk = 28          // Android 9: the first version that can copy an installed app into a work profile
        targetSdk = 34
        versionCode = 1
        versionName = "0.1.0"
    }

    // One fixed signing key, so every build can update the one before it.
    // (This key is public in the repo on purpose: fine for a hobby app you install yourself, not for the Play Store.)
    signingConfigs {
        create("shared") {
            storeFile = file("../dualspace.jks")
            storePassword = "dualspace"
            keyAlias = "dualspace"
            keyPassword = "dualspace"
        }
    }

    buildTypes {
        debug {
            signingConfig = signingConfigs.getByName("shared")
        }
        release {
            signingConfig = signingConfigs.getByName("shared")
            isMinifyEnabled = false
        }
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }

    lint {
        abortOnError = false
    }
}
