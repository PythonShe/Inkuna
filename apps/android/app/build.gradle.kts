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

val inkunaAbis = (findProperty("inkunaAbis") as String? ?: "arm64-v8a,x86_64")
    .split(",")
    .map(String::trim)
    .filter(String::isNotEmpty)

android {
    namespace = "app.inkuna.android"
    compileSdk = 37

    defaultConfig {
        applicationId = "app.inkuna.android"
        minSdk = 33
        targetSdk = 37
        versionCode = 26081601
        versionName = "0.5.3"

        // Mirrors ANDROID_ABIS in scripts/build-core-android.sh: local builds
        // package the x86_64 emulator slice too, CI releases pass
        // -PinkunaAbis=arm64-v8a. minSdk 33 has no 32-bit-only devices, so
        // armeabi-v7a is never built — the filter also drops the armeabi-v7a
        // and x86 slices that JNA and the AndroidX libs bundle.
        ndk {
            abiFilters += inkunaAbis
        }
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
        // Readium's AARs declare java.time/NIO usage that needs desugaring
        // on device even at minSdk 33.
        isCoreLibraryDesugaringEnabled = true
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
    // The reader hosts Readium's fragment-based EPUB navigator in Compose
    // and drives the core contract from a ViewModel.
    implementation("androidx.fragment:fragment-ktx:1.9.0")
    implementation("androidx.lifecycle:lifecycle-runtime-compose:2.11.0")
    implementation("androidx.lifecycle:lifecycle-viewmodel-compose:2.11.0")
    // Readium renders and paginates; the Rust core owns storage, metadata,
    // and progress behind the FFI and never renders.
    implementation("org.readium.kotlin-toolkit:readium-shared:3.3.0")
    implementation("org.readium.kotlin-toolkit:readium-streamer:3.3.0")
    implementation("org.readium.kotlin-toolkit:readium-navigator:3.3.0")
    // UniFFI-generated bindings load the Rust core through JNA.
    implementation("net.java.dev.jna:jna:5.19.1@aar")
    coreLibraryDesugaring("com.android.tools:desugar_jdk_libs:2.1.5")
}
