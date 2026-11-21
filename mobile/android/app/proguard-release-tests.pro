# Added to the app's R8 rules only when the instrumented tests run on the
# minified release build (-Plectio.testRelease=true, ADR-038); the build that
# ships doesn't have these.

# The test APK takes the libraries it shares with the app (Kotlin, coroutines,
# AndroidX, Compose) from the app, so R8 must not remove the parts only the
# tests call (P-071).
-keep class kotlin.** { *; }
-keep class kotlinx.** { *; }
-keep class androidx.** { *; }

# The tests call into the app (test set-up such as startForTests(), the test
# tone, view classes); keep the app's own classes as they are. JNA, the
# UniFFI bindings and every other library are still shrunk as in the release.
-keep class org.lectio.android.** { *; }
