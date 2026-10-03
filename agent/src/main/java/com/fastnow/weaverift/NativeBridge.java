package com.fastnow.weaverift;
public final class NativeBridge {
    public static native void registerBridge(Class<?> bridgeClass);
    private NativeBridge() {}
}