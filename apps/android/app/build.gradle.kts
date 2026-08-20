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
        versionCode = 26081902
        versionName = "0.8.2"

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
            // R8 is what makes material-icons-extended affordable: the artifact
            // dexes ~8.5k icons x 5 themes (~78 MB of dex), of which this app
            // references 23. Without shrinking they all ship. Keep rules for
            // the reflective JNA/UniFFI boundary live in proguard-rules.pro.
            isMinifyEnabled = true
            isShrinkResources = true
            proguardFiles(
                getDefaultProguardFile("proguard-android-optimize.txt"),
                "proguard-rules.pro",
            )
        }
    }

    packaging {
        // The APK is distributed standalone via GitHub releases rather than
        // as a Play AAB, so download size beats install footprint: compress
        // the 14 MB Rust core instead of storing it page-aligned. Costs a
        // second on-device copy at install time (extractNativeLibs=true).
        jniLibs.useLegacyPackaging = true
    }

    buildFeatures {
        compose = true
        // The settings sheet's footer and the update checker read
        // BuildConfig.VERSION_NAME / VERSION_CODE.
        buildConfig = true
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
    // Referenced directly by BoundaryDragFollower to fake-drag Readium's
    // resource pager across chapter boundaries.
    implementation("androidx.viewpager:viewpager:1.1.0")
    // Document-start JS injection for the reader's own user stylesheet.
    // Capped below 1.17.0 on purpose: from 1.17.0 on,
    // WebViewAssetLoader.AssetsPathHandler builds its WebResourceResponse with
    // the shared immutable Map.of("Cache-Control", …) header map, and Readium
    // 3.3.0's WebViewServer.allowCors() puts "Access-Control-Allow-Origin"
    // into that map in place (WebViewServer.kt:263). Every served asset — our
    // bundled Noto fonts included — then throws UnsupportedOperationException
    // on the main thread the moment a book opens. `strictly` so a transitive
    // bump cannot reintroduce it; lift once Readium copies the headers first.
    implementation("androidx.webkit:webkit") { version { strictly("1.16.0") } }
    implementation("androidx.lifecycle:lifecycle-runtime-compose:2.11.0")
    // The evening reminder: WorkManager survives reboots without a boot
    // receiver, so the daily nudge is one self-chaining worker.
    implementation("androidx.work:work-runtime-ktx:2.11.2")
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
