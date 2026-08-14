import java.util.Properties

plugins {
    id("com.android.application")
    id("org.jetbrains.kotlin.plugin.compose")
}

// Release signing material lives outside git: CI (release-android.yml)
// materializes key.properties + the keystore from repository secrets; both
// are gitignored. Without them, release builds fall back to debug signing so
// local `assembleRelease` still works.
val keyProps = Properties().apply {
    val f = rootProject.file("key.properties")
    if (f.exists()) f.inputStream().use { load(it) }
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

    if (keyProps.isNotEmpty()) {
        signingConfigs {
            create("release") {
                // storeFile resolves relative to this module (app/).
                storeFile = file(keyProps.getProperty("storeFile"))
                storePassword = keyProps.getProperty("storePassword")
                keyAlias = keyProps.getProperty("keyAlias")
                keyPassword = keyProps.getProperty("keyPassword")
            }
        }
    }

    buildTypes {
        release {
            signingConfig = if (keyProps.isNotEmpty()) {
                signingConfigs.getByName("release")
            } else {
                signingConfigs.getByName("debug")
            }
        }
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
    // Material Symbols equivalents for the design system's icon set.
    implementation("androidx.compose.material:material-icons-extended:1.7.8")
    implementation("androidx.navigation:navigation-compose:2.9.8")
    implementation("androidx.datastore:datastore-preferences:1.2.1")
    // UniFFI-generated bindings load the Rust core through JNA.
    implementation("net.java.dev.jna:jna:5.19.1@aar")
}
