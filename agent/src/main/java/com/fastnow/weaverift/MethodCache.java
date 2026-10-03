package com.fastnow.weaverift;

import java.lang.reflect.Method;
import java.util.Map;
import java.util.concurrent.ConcurrentHashMap;

public final class MethodCache {

    private static final Map<String, Method> CACHE = new ConcurrentHashMap<>();

    private static VersionConfig config;
    private static NamespaceResolver resolver;

    public static void init(VersionConfig cfg, NamespaceResolver res) {
        config = cfg;
        resolver = res;
    }

    public static Method get(String logicalKey) {
        Method cached = CACHE.get(logicalKey);
        if (cached != null) {
            return cached;
        }
        Method resolved = resolve(logicalKey);
        if (resolved != null) {
            CACHE.put(logicalKey, resolved);
        }
        return resolved;
    }

    public static Object invokeStatic(String logicalKey) {
        Method m = get(logicalKey);
        if (m == null) {
            return null;
        }
        try {
            return m.invoke(null);
        } catch (Throwable t) {
            log("静态方法调用失败: " + logicalKey + " - " + t);
            return null;
        }
    }

    public static Object invokeStatic(String logicalKey, Object... args) {
        Method m = get(logicalKey);
        if (m == null) {
            return null;
        }
        try {
            return m.invoke(null, args);
        } catch (Throwable t) {
            log("静态方法调用失败: " + logicalKey + " - " + t);
            return null;
        }
    }

    public static Object invoke(String logicalKey, Object target) {
        Method m = get(logicalKey);
        if (m == null) {
            return null;
        }
        try {
            return m.invoke(target);
        } catch (Throwable t) {
            log("方法调用失败: " + logicalKey + " - " + t);
            return null;
        }
    }

    private static Method resolve(String logicalKey) {
        if (config == null) {
            log("MethodCache 未初始化: " + logicalKey);
            return null;
        }
        VersionConfig.MethodDef def = config.methods().get(logicalKey);
        if (def == null) {
            log("方法未定义: " + logicalKey);
            return null;
        }
        Class<?> owner = ClassCache.get(def.owner);
        if (owner == null) {
            log("方法 owner 类未找到: " + logicalKey + " (owner: " + def.owner + ")");
            return null;
        }
        VersionConfig.ClassDef ownerDef = config.classes().get(def.owner);
        if (ownerDef == null) {
            log("方法 owner 未在 classes 段定义: " + def.owner);
            return null;
        }

        WeaveRiftAgent.MethodMapping mm =
                resolver.mapMethod(ownerDef.srg, def.srg, def.sig);
        String actualName = mm.obfName;
        String actualSig = mm.obfSig;

        Class<?> c = owner;
        while (c != null) {
            Method[] methods = c.getDeclaredMethods();
            for (Method m : methods) {
                if (!m.getName().equals(actualName)) {
                    continue;
                }
                if (actualSig != null && !actualSig.isEmpty()
                        && !signatureMatches(m, actualSig)) {
                    continue;
                }
                m.setAccessible(true);
                log("Method " + logicalKey + " -> " + c.getSimpleName()
                        + "." + actualName + sigOf(m));
                return m;
            }
            c = c.getSuperclass();
        }
        log("方法找不到: " + logicalKey + " (resolved: " + actualName + actualSig + ")");
        return null;
    }

    private static boolean signatureMatches(Method m, String sig) {
        if (sig == null || sig.isEmpty()) {
            return true;
        }
        return sig.equals(sigOf(m));
    }

    private static String sigOf(Method m) {
        StringBuilder sb = new StringBuilder("(");
        for (Class<?> p : m.getParameterTypes()) {
            sb.append(typeSig(p));
        }
        sb.append(')');
        sb.append(typeSig(m.getReturnType()));
        return sb.toString();
    }

    private static String typeSig(Class<?> c) {
        if (c == void.class) return "V";
        if (c == boolean.class) return "Z";
        if (c == byte.class) return "B";
        if (c == char.class) return "C";
        if (c == short.class) return "S";
        if (c == int.class) return "I";
        if (c == long.class) return "J";
        if (c == float.class) return "F";
        if (c == double.class) return "D";
        if (c.isArray()) {
            return "[" + typeSig(c.getComponentType());
        }
        return "L" + c.getName().replace('.', '/') + ";";
    }

    private static void log(String s) {
        System.out.println("[WeaveRift] " + s);
    }

    private MethodCache() {}
}