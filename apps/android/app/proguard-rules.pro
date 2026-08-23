# R8 keep rules for the release build.
#
# Almost everything here exists because something crosses a reflective
# boundary R8 cannot see: JNA resolves native symbols and struct layouts by
# reflection. Shrinking itself is what makes
# material-icons-extended affordable (see build.gradle.kts) — nothing below
# should ever grow to cover Compose or the icon set.

# --- JNA -------------------------------------------------------------------
# JNA binds native functions by matching Java method names to exported
# symbols, and maps Structure subclasses field-by-field via reflection, so
# names and members must survive on both. JNA also references a handful of
# java.awt types that do not exist on Android; they are unreachable here.
-keep class com.sun.jna.** { *; }
-keep interface com.sun.jna.** { *; }
-keepclassmembers class * extends com.sun.jna.Structure {
    <fields>;
    <methods>;
}
-keep class * implements com.sun.jna.Callback { *; }
-dontwarn java.awt.**

# --- UniFFI generated bindings ---------------------------------------------
# app.inkuna.core is generated wholesale by scripts/build-core-android.sh and
# is the entire FFI surface: JNA Library method names must match the Rust
# exports byte-for-byte, RustBuffer/UniffiRustCallStatus/UniffiVTable* are
# @Structure.FieldOrder types read by field name, and the callback interfaces
# are invoked from Rust. Renaming or pruning any of it breaks at the first
# core call. The package is ~4.5k lines, so keeping all of it is cheap.
-keep class app.inkuna.core.** { *; }
-keep interface app.inkuna.core.** { *; }
