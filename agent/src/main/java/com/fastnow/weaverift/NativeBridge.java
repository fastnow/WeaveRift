package com.fastnow.weaverift;

public class NativeBridge {
    static {
        try {
            String dllPath = System.getProperty("weaverift.dll");
            if (dllPath != null) {
                System.load(dllPath);
                System.out.println("[WeaveRift] NativeBridge: loaded " + dllPath);
            } else {
                System.out.println("[WeaveRift] NativeBridge: weaverift.dll 属性未设置");
            }
        } catch (Throwable t) {
            System.out.println("[WeaveRift] NativeBridge: System.load 失败");
            t.printStackTrace();
        }
    }

    public static native void registerBridge(Class<?> bridgeClass);
}