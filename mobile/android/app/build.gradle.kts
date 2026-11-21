import java.util.Properties

// The Lectio Android app (ADR-033). Before building, run
// tools/mobile/build-android.sh: it puts the Rust libraries in
// src/main/jniLibs and the Kotlin bindings in src/generated/kotlin.
plugins {
    alias(libs.plugins.android.application)
    alias(libs.plugins.kotlin.compose)
}


fun properties(text: String) = Properties().apply { load(text.reader()) }

/** Version and store build number: mobile/version.properties, shared with iOS (ADR-036). */
val versions = properties(providers.fileContents(rootProject.layout.projectDirectory.file("../version.properties")).asText.get())
val lectioVersion: String = versions.getProperty("version")
val lectioBuild: Int = versions.getProperty("build").toInt()

/**
 * The upload / release key (ADR-035, ADR-038), never in the repository: from
 * the environment (CI secrets), or from a gitignored keystore.properties next
 * to settings.gradle.kts (storeFile, storePassword, keyAlias, keyPassword).
 */
val localSigning = rootProject.file("keystore.properties").takeIf { it.exists() }?.let { properties(it.readText()) }
val releaseKeystore: String? = (System.getenv("LECTIO_KEYSTORE_FILE") ?: localSigning?.getProperty("storeFile"))
    ?.takeIf { it.isNotBlank() && rootProject.file(it).exists() }
fun signingValue(env: String, key: String): String? = System.getenv(env) ?: localSigning?.getProperty(key)

/** -Plectio.testRelease=true: run the instrumented tests on the minified release build. */
val testsOnRelease = providers.gradleProperty("lectio.testRelease").orNull == "true"

android {
    namespace = "org.lectio.android"
    // Compose (BOM 2026.11) is built against Android 37.1; Gradle downloads
    // the platform if the SDK doesn't have it yet.
    compileSdk = 37
    compileSdkMinor = 1

    defaultConfig {
        // The store identity (ADR-036); the Kotlin package stays org.lectio.android.
        applicationId = "io.github.mehmetefeozdemir.lectio"
        minSdk = 26
        targetSdk = 37
        versionName = lectioVersion
        versionCode = lectioBuild
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
                storeFile = rootProject.file(releaseKeystore)
                storePassword = signingValue("LECTIO_KEYSTORE_PASSWORD", "storePassword")
                keyAlias = signingValue("LECTIO_KEY_ALIAS", "keyAlias")
                keyPassword = signingValue("LECTIO_KEY_PASSWORD", "keyPassword")
            }
        }
    }

    buildTypes {
        release {
            // R8 shrinks the release build (mostly unused icons and library
            // code); proguard-rules.pro keeps what JNA and the UniFFI bindings
            // find by name. The instrumented tests run against this build with
            // -Plectio.testRelease=true (ADR-038).
            isMinifyEnabled = true
            isShrinkResources = true
            proguardFiles(getDefaultProguardFile("proguard-android-optimize.txt"), "proguard-rules.pro")
            testProguardFiles("proguard-test-rules.pro")
            if (testsOnRelease) proguardFile("proguard-release-tests.pro")
            // Without the secrets the release APK is signed with the debug
            // key, so it can still be installed by hand.
            signingConfig = signingConfigs.findByName("release") ?: signingConfigs.getByName("debug")
        }
    }

    // The instrumented tests normally run on the debug build; this runs them
    // on the minified release build instead.
    testBuildType = if (testsOnRelease) "release" else "debug"

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
    // The empty activity some Compose tests host their content in; in the
    // release build only while testing it, never in the one that ships.
    if (testsOnRelease) releaseImplementation(libs.compose.ui.test.manifest)
}
