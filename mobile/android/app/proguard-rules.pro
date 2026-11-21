# R8 rules for the release build (ADR-038).

# JNA calls back into Java from native code and reads classes, fields and
# methods by name; renaming or removing any of them breaks every Rust call.
-keep class com.sun.jna.** { *; }
-keepclassmembers class * extends com.sun.jna.** { public *; }
-dontwarn java.awt.**

# The UniFFI bindings (uniffi.lectio_ffi): JNA structures, callback
# interfaces and the native method table are looked up by name.
-keep class uniffi.** { *; }
