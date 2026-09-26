package com.fastnow.weaverift;

public class NativeBridge {
    static {
        try {
            // 从系统属性拿 DLL 路径
            String dllPath = System.getProperty("weaverift.dll");
            if (dllPath != null) {
                System.load(dllPath);
                System.out.println("[WeaveRift] Loaded native lib: " + dllPath);
            } else {
                System.out.println("[WeaveRift] weaverift.dll not set, native methods unavailable");
            }
        } catch (Throwable t) {
            System.out.println("[WeaveRift] Failed to load native lib:");
            t.printStackTrace();
        }
    }

    public static native int redefineClass(String className, byte[] newBytes);
}