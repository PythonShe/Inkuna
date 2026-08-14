plugins {
    id("com.android.application")
    id("org.jetbrains.kotlin.plugin.compose")
}

android {
    namespace = "app.inkuna.android"
    compileSdk = 37

    defaultConfig {
        applicationId = "app.inkuna.android"
        minSdk = 33
        targetSdk = 37
        versionCode = 1
        versionName = "0.1.0"
    }

    sourceSets {
        // UniFFI bindings emitted by scripts/build-core-android.sh.
        getByName("main").kotlin.srcDir("src/generated/kotlin")
    }

    buildFeatures {
        compose = true
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }
}

dependencies {
    implementation(platform("androidx.compose:compose-bom:2026.08.00"))
    implementation("androidx.activity:activity-compose:1.13.0")
    implementation("androidx.compose.material3:material3")
    implementation("androidx.compose.ui:ui-tooling-preview")
    // UniFFI-generated bindings load the Rust core through JNA.
    implementation("net.java.dev.jna:jna:5.19.1@aar")
}
