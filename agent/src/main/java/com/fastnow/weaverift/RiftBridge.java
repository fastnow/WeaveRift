package com.fastnow.weaverift;

import java.util.concurrent.atomic.AtomicReference;

public final class RiftBridge {

    private static final AtomicReference<double[]> SNAPSHOT =
            new AtomicReference<>(new double[9]);
    private static final AtomicReference<String> ERROR =
            new AtomicReference<>("");

    private static volatile boolean mouseGrabbed = true;

    public static double[] snapshotArray() {
        return SNAPSHOT.get();
    }

    public static String getError() {
        return ERROR.get();
    }

    public static void setError(String msg) {
        ERROR.set(msg == null ? "" : msg);
    }

    public static String onCommand(String cmd) {
        if (cmd == null) return "null";
        switch (cmd) {
            case "status":
                return "ents=" + ((SNAPSHOT.get().length - 9) / 6);
            case "error":
                return ERROR.get();
            default:
                return "unknown: " + cmd;
        }
    }

    public static void updateSnapshot(double valid,
                                      double x, double y, double z,
                                      double yaw, double pitch,
                                      double health, double maxHealth,
                                      double entities) {
        double[] arr = new double[9];
        arr[0] = valid;
        arr[1] = x;
        arr[2] = y;
        arr[3] = z;
        arr[4] = yaw;
        arr[5] = pitch;
        arr[6] = health;
        arr[7] = maxHealth;
        arr[8] = entities;
        SNAPSHOT.set(arr);
    }

    public static void updateSnapshotFull(double[] arr) {
        SNAPSHOT.set(arr);
    }

    public static void setMouseGrabbedState(boolean v) {
        mouseGrabbed = v;
    }

    public static boolean isMouseGrabbed() {
        return mouseGrabbed;
    }

    private RiftBridge() {}
}