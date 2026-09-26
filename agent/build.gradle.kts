plugins {
    java
}

group = "com.fastnow.weaverift"
version = "1.0.0"

java {
    sourceCompatibility = JavaVersion.VERSION_21
    targetCompatibility = JavaVersion.VERSION_21
}

tasks.jar {
    manifest {
        attributes(
            "Agent-Class" to "com.fastnow.weaverift.WeaveRiftAgent",
            "Premain-Class" to "com.fastnow.weaverift.WeaveRiftAgent",
            "Can-Redefine-Classes" to "true",
            "Can-Retransform-Classes" to "true"
        )
    }
}

dependencies {
    implementation("org.ow2.asm:asm:9.7")
    implementation("org.ow2.asm:asm-commons:9.7")
    implementation("org.lwjgl.lwjgl:lwjgl:2.9.3")
}