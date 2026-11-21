// The Lectio Android app (ADR-033). Before building, run
// tools/mobile/build-android.sh: it puts the Rust libraries in
// src/main/jniLibs and the Kotlin bindings in src/generated/kotlin.
plugins {
    alias(libs.plugins.android.application)
    alias(libs.plugins.kotlin.compose)
}

/** "0.5.0" → 500; the version code only has to grow between releases. */
fun versionCodeOf(name: String): Int =
    name.split('.').map { it.takeWhile(Char::isDigit).toIntOrNull() ?: 0 }
        .let { (it + listOf(0, 0, 0)).take(3) }
        .let { (major, minor, patch) -> major * 10_000 + minor * 100 + patch }

val lectioVersion = providers.gradleProperty("lectio.version").get()

/** Release signing comes only from the environment (CI secrets); see ADR-035. */
val releaseKeystore: String? = System.getenv("LECTIO_KEYSTORE_FILE")?.takeIf { it.isNotBlank() && file(it).exists() }

android {
    namespace = "org.lectio.android"
    // Compose (BOM 2026.11) is built against Android 37.1; Gradle downloads
    // the platform if the SDK doesn't have it yet.
    compileSdk = 37
    compileSdkMinor = 1

    defaultConfig {
        applicationId = "org.lectio.android"
        minSdk = 26
        targetSdk = 37
        versionName = lectioVersion
        versionCode = versionCodeOf(lectioVersion)
        // Only the ABIs the Rust core is built for (JNA alone would add x86,
        // mips and armeabi, where Lectio couldn't load its library).
        ndk { abiFilters += listOf("arm64-v8a", "armeabi-v7a", "x86_64") }
        testInstrumentationRunner = "androidx.test.runner.AndroidJUnitRunner"
        // Screenshots from instrumented tests go through the test storage
        // service, so Gradle copies them to build/outputs/.
        testInstrumentationRunnerArguments["useTestStorageService"] = "true"
    }

    signingConfigs {
        if (releaseKeystore != null) {
            create("release") {
                storeFile = file(releaseKeystore)
                storePassword = System.getenv("LECTIO_KEYSTORE_PASSWORD")
                keyAlias = System.getenv("LECTIO_KEY_ALIAS")
                keyPassword = System.getenv("LECTIO_KEY_PASSWORD")
            }
        }
    }

    buildTypes {
        release {
            // R8 is off: JNA and the generated bindings look classes up by
            // name, and the gain is size only.
            isMinifyEnabled = false
            // Without the secrets the release APK is signed with the debug
            // key, so it can still be installed by hand.
            signingConfig = signingConfigs.findByName("release") ?: signingConfigs.getByName("debug")
        }
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }

    buildFeatures {
        compose = true
    }

    sourceSets {
        getByName("main") {
            kotlin.directories.add("src/generated/kotlin")
        }
    }

    testOptions {
        unitTests.all {
            // The JVM tests call the real Rust core, built for this computer
            // by build-android.sh, through JNA.
            it.systemProperty("jna.library.path", rootProject.file("../../target/debug").absolutePath)
        }
    }

    packaging {
        jniLibs.useLegacyPackaging = false
    }
}

dependencies {
    implementation(libs.androidx.core.ktx)
    implementation(libs.androidx.activity.compose)
    implementation(libs.androidx.lifecycle.viewmodel.compose)
    implementation(libs.androidx.lifecycle.runtime.compose)
    implementation(libs.androidx.work.runtime.ktx)
    implementation(libs.kotlinx.coroutines.android)
    implementation(platform(libs.compose.bom))
    implementation(libs.compose.ui)
    implementation(libs.compose.ui.graphics)
    implementation(libs.compose.ui.tooling.preview)
    implementation(libs.compose.material3)
    implementation(libs.compose.material.icons.extended)
    // UniFFI's Kotlin bindings call Rust through JNA: the Android build
    // (with its native dispatch library) for the app, the plain jar for the
    // JVM tests.
    implementation(libs.jna) { artifact { type = "aar" } }
    debugImplementation(libs.compose.ui.tooling)

    testImplementation(libs.jna)
    testImplementation(libs.junit)
    testImplementation(libs.kotlinx.coroutines.test)

    androidTestImplementation(platform(libs.compose.bom))
    androidTestImplementation(libs.compose.ui.test.junit4)
    androidTestImplementation(libs.androidx.test.core)
    androidTestImplementation(libs.androidx.test.runner)
    androidTestImplementation(libs.androidx.test.rules)
    androidTestImplementation(libs.androidx.test.ext.junit)
    androidTestImplementation(libs.androidx.test.storage)
    androidTestImplementation(libs.kotlinx.coroutines.test)
    androidTestUtil(libs.androidx.test.services)
    debugImplementation(libs.compose.ui.test.manifest)
}
