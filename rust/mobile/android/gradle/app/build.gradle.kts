// leanote Android app module（issue 10）：打包预编译 Rust cdylib 为 APK。
// Rust 库先经 cargo-ndk 编进 jniLibs（见 build.sh）。
plugins {
    id("com.android.application")
}

android {
    namespace = "com.leanote.mobile"
    compileSdk = 34

    defaultConfig {
        applicationId = "com.leanote.mobile"
        minSdk = 26
        targetSdk = 34
        versionCode = 1
        versionName = "1.0.0"

        ndk {
            abiFilters += listOf("arm64-v8a")
        }
        manifestPlaceholders["nativeLibraryName"] = "leanote_mobile"
    }

    buildTypes {
        release {
            isMinifyEnabled = false
        }
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }
}

dependencies {
    implementation("androidx.core:core:1.12.0")
    implementation("androidx.core:core-splashscreen:1.0.1")
    implementation("androidx.biometric:biometric:1.1.0")
    implementation("androidx.media:media:1.7.1")
    implementation("androidx.exifinterface:exifinterface:1.3.7")
    implementation("androidx.webkit:webkit:1.11.0")
}
