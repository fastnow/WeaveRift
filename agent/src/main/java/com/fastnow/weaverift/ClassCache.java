package com.fastnow.weaverift;

import java.util.Map;
import java.util.concurrent.ConcurrentHashMap;

public final class ClassCache {

    private static final Map<String, Class<?>> CACHE = new ConcurrentHashMap<>();

    private static ClassLoader gameCl;
    private static VersionConfig config;
    private static NamespaceResolver resolver;

    public static void init(ClassLoader cl, VersionConfig cfg, NamespaceResolver res) {
        gameCl = cl;
        config = cfg;
        resolver = res;
    }

    public static Class<?> get(String logicalKey) {
        Class<?> cached = CACHE.get(logicalKey);
        if (cached != null) {
            return cached;
        }
        Class<?> resolved = resolve(logicalKey);
        if (resolved != null) {
            CACHE.put(logicalKey, resolved);
        }
        return resolved;
    }

    private static Class<?> resolve(String logicalKey) {
        if (config == null) {
            log("ClassCache 未初始化: " + logicalKey);
            return null;
        }
        VersionConfig.ClassDef def = config.classes().get(logicalKey);
        if (def == null) {
            log("类未定义: " + logicalKey);
            return null;
        }
        String actualName = resolver.mapClass(def.srg);
        try {
            Class<?> c = Class.forName(actualName, false, gameCl);
            log("Class " + logicalKey + " -> " + c.getName());
            return c;
        } catch (Throwable t) {
            log("类加载失败: " + logicalKey + " (resolved: " + actualName + ") - " + t);
            return null;
        }
    }

    private static void log(String s) {
        System.out.println("[WeaveRift] " + s);
    }

    private ClassCache() {}
}