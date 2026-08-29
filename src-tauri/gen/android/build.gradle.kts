buildscript {
    repositories {
        google()
        mavenCentral()
    }
    dependencies {
        classpath("com.android.tools.build:gradle:8.11.0")
        classpath("org.jetbrains.kotlin:kotlin-gradle-plugin:1.9.25")
    }
}

allprojects {
    repositories {
        google()
        mavenCentral()
    }
}

subprojects {
    // DEV-SYNC-003-F2 · QR 识别 OFFLINE FIRST（§四/§六）：
    // tauri-plugin-barcode-scanner 2.4.5 的 android 模块（cargo registry，禁改）只声明
    // GMS 动态模型版 play-services-mlkit-barcode-scanning:18.1.0 —— 无 Google Play
    // Services 的真机上模型无法下载 → CameraX 出画面但分析器 0 结果（scan 永不 resolve）。
    // 修复 = 为插件模块与 app 注入 bundled 版 com.google.mlkit:barcode-scanning:17.3.0：
    //   - 识别模型 .tflite/.so 随 APK 打包（完全离线，MLKIT_BUNDLED_GATE 实证）；
    //   - 运行时 ML Kit 检测到 ThickBarcodeScannerCreator（bundled）优先走本地模型；
    //   - Gradle 解析插件 18.1.0 与 bundled 传递 18.3.1 时取高版本（18.3.1）。
    // 结构实证（官方 POM + AAR 字节级校验，SHA1 一致）：
    //   bundled 17.3.0 POM 自身 compile 依赖 play-services-mlkit-barcode-scanning:18.3.1 ——
    //   该 artifact 是唯一提供 API 门面（BarcodeScanning/BarcodeScanner/BarcodeScannerOptions）
    //   的层（三件套类不存在于 mlkit 组任何 AAR）。「graph 完全不含 gms 坐标」在 ML Kit
    //   官方架构下不可实现（exclude 它 = 连官方 bundled 也无法编译）；本任务 OFFLINE FIRST
    //   的硬指标 = 模型随 APK + 无 GMS 可识别，由 MLKIT_BUNDLED_GATE（依赖图 + APK 内
    //   模型文件）保证。
    // 本文件为 canonical Gradle source（Step 10.8 只再生成 tauri.settings.gradle /
    // tauri.build.gradle.kts，不触碰本文件）→ 连续构建持久。
    plugins.withId("com.android.library") {
        if (name == "tauri-plugin-barcode-scanner") {
            dependencies.add("implementation", "com.google.mlkit:barcode-scanning:17.3.0")
        }
    }
    plugins.withId("com.android.application") {
        dependencies.add("implementation", "com.google.mlkit:barcode-scanning:17.3.0")
    }
}

tasks.register("clean").configure {
    delete("build")
}

