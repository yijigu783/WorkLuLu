buildscript {
    repositories {
        // 官方源实测只有几 KB/s，国内镜像优先（见本脚本顶部说明）
        maven("https://maven.aliyun.com/repository/google")
        maven("https://maven.aliyun.com/repository/central")
        maven("https://maven.aliyun.com/repository/gradle-plugin")
        // google() 在本机连不通，换成等价内容源（见本脚本顶部说明）
        maven("https://dl.google.com/dl/android/maven2")
        mavenCentral()
    }
    dependencies {
        classpath("com.android.tools.build:gradle:9.3.1")
        classpath("org.jetbrains.kotlin:kotlin-gradle-plugin:2.2.10")
    }
}

allprojects {
    repositories {
        // 官方源实测只有几 KB/s，国内镜像优先（见本脚本顶部说明）
        maven("https://maven.aliyun.com/repository/google")
        maven("https://maven.aliyun.com/repository/central")
        maven("https://maven.aliyun.com/repository/gradle-plugin")
        // google() 在本机连不通，换成等价内容源（见本脚本顶部说明）
        maven("https://dl.google.com/dl/android/maven2")
        mavenCentral()
    }
}

tasks.register("clean").configure {
    delete("build")
}

