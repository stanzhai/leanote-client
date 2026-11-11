// leanote Android 根工程（issue 10）：Rust cdylib + APK 一条龙。
plugins {
    id("com.android.application") version "8.5.2" apply false
}

tasks.register("clean", Delete::class) {
    delete(rootProject.layout.buildDirectory)
}

// workingDir = （gradle 在 mobile/android/gradle）
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
        // workingDir = gradle 根（mobile/android/gradle）；-o 相对此处，
        // 此前写成 repo 根相对路径，产物落到嵌套的不存在目录、APK 打进
        // 旧 .so（修复：从此目录到 jniLibs 的正确相对路径）。
        "-o", "app/src/main/jniLibs",
        "build", "--lib", "-p", "leanote-mobile",
    )
}
