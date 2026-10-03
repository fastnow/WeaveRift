package com.fastnow.weaverift;

import java.lang.reflect.Field;
import java.util.Map;
import java.util.concurrent.ConcurrentHashMap;

public final class FieldCache {

    private static final Map<String, Field> CACHE = new ConcurrentHashMap<>();

    private static VersionConfig config;
    private static NamespaceResolver resolver;

    public static void init(VersionConfig cfg, NamespaceResolver res) {
        config = cfg;
        resolver = res;
    }

    public static Field get(String logicalKey) {
        Field cached = CACHE.get(logicalKey);
        if (cached != null) {
            return cached;
        }
        Field resolved = resolve(logicalKey);
        if (resolved != null) {
            CACHE.put(logicalKey, resolved);
        }
        return resolved;
    }

    private static Field resolve(String logicalKey) {
        if (config == null) {
            log("FieldCache 未初始化: " + logicalKey);
            return null;
        }
        VersionConfig.FieldDef def = config.fields().get(logicalKey);
        if (def == null) {
            log("字段未定义: " + logicalKey);
            return null;
        }
        Class<?> owner = ClassCache.get(def.owner);
        if (owner == null) {
            log("字段 owner 类未找到: " + logicalKey + " (owner: " + def.owner + ")");
            return null;
        }
        VersionConfig.ClassDef ownerDef = config.classes().get(def.owner);
        if (ownerDef == null) {
            log("字段 owner 未在 classes 段定义: " + def.owner);
            return null;
        }
        String actualName = resolver.mapField(ownerDef.srg, def.srg);
        Class<?> c = owner;
        while (c != null) {
            try {
                Field f = c.getDeclaredField(actualName);
                f.setAccessible(true);
                log("Field " + logicalKey + " -> " + c.getSimpleName() + "." + actualName);
                return f;
            } catch (NoSuchFieldException ignored) {
            } catch (Throwable t) {
                log("字段访问异常: " + logicalKey + " - " + t);
                return null;
            }
            c = c.getSuperclass();
        }
        log("字段找不到: " + logicalKey + " (resolved: " + actualName + ")");
        return null;
    }

    private static void log(String s) {
        System.out.println("[WeaveRift] " + s);
    }

    private FieldCache() {}
}