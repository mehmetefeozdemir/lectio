# R8 rules for the test APK when the instrumented tests run on the minified
# release build (ADR-038).

# Annotations in the test libraries refer to compile-time-only classes.
-dontwarn javax.lang.model.**
-dontwarn com.google.errorprone.annotations.**
