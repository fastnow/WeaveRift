package com.fastnow.weaverift;

import java.util.concurrent.atomic.AtomicReference;

/**
 * native 侧的数据出口。
 *
 * <p>native 每帧（经 Sampler 节流到 ~20Hz）用 JNI GetEnv 调 {@link #snapshotArray()}，
 * 不 attach 线程、不建本地引用以外的东西，读不到就返回 valid=0。
 *
 * <p>⚠ 数组长度变了必须同步改 jni_bridge.rs 里的 buf 长度（目前 9）。
 */
public final class RiftBridge {

    /** [valid, x, y, z, yaw, pitch, health, maxHealth, entities] */
    private static final AtomicReference<double[]> SNAPSHOT =
            new AtomicReference<>(new double[9]);

    private static volatile String lastError = "";

    private RiftBridge() {}

    public static double[] snapshotArray() {
        return SNAPSHOT.get();
    }

    /** 由 {@link WeaveRiftAgent} 的采样线程调用；复用数组，避免每次分配 */
    static void updateSnapshot(double valid, double x, double y, double z,
                               double yaw, double pitch,
                               double health, double maxHealth, double entities) {
        double[] buf = new double[] {
                valid, x, y, z, yaw, pitch, health, maxHealth, entities
        };
        SNAPSHOT.set(buf);
    }

    static void setError(String e) {
        lastError = e == null ? "" : e;
    }

    /**
     * 反向控制入口：native 调试台 -> Java。
     * 返回值会显示在控制台上。
     */
    public static String onCommand(String cmd) {
        if (cmd == null) return "null";
        switch (cmd) {
            case "rescan": {
                boolean ok = WeaveRiftAgent.reloadMappings();
                return ok ? "mappings reloaded" : "reload failed: " + lastError;
            }
            case "status":
                return WeaveRiftAgent.describe();
            case "error":
                return lastError.isEmpty() ? "(none)" : lastError;
            default:
                return "unknown: " + cmd;
        }
    }
}
