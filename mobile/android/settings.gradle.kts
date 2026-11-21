// Lectio for Android (ADR-033). The Rust core is built first by
// tools/mobile/build-android.sh (native libraries + Kotlin bindings).
pluginManagement {
    repositories {
        google()
        mavenCentral()
        gradlePluginPortal()
    }
}

dependencyResolutionManagement {
    repositoriesMode.set(RepositoriesMode.FAIL_ON_PROJECT_REPOS)
    repositories {
        google()
        mavenCentral()
    }
}

rootProject.name = "Lectio"
include(":app")
