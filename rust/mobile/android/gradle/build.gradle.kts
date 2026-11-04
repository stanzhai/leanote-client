// leanote Android 根工程（issue 10）：Rust cdylib + APK 一条龙。
plugins {
    id("com.android.application") version "8.5.2" apply false
}

tasks.register("clean", Delete::class) {
    delete(rootProject.layout.buildDirectory)
}

// workingDir = rust/（gradle 在 rust/mobile/android/gradle）
tasks.register<Exec>("buildRustRelease") {
    group = "rust"
    commandLine(
        "cargo", "ndk",
        "-t", "arm64-v8a",
        "-P", "26",
        "-o", "mobile/android/gradle/app/src/main/jniLibs",
        "build", "--lib", "-p", "leanote-mobile", "--release",
    )
}

tasks.register<Exec>("buildRustDebug") {
    group = "rust"
    commandLine(
        "cargo", "ndk",
        "-t", "arm64-v8a",
        "-P", "26",
        "-o", "mobile/android/gradle/app/src/main/jniLibs",
        "build", "--lib", "-p", "leanote-mobile",
    )
}
