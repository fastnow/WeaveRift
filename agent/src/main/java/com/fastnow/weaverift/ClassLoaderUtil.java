package com.fastnow.weaverift;

import java.lang.instrument.Instrumentation;
import java.lang.reflect.Field;

/**
 * 定位 Minecraft 类所在的 ClassLoader。
 *
 * <p>★ agent jar 挂在 system classloader 上，而 MC 类在 LaunchClassLoader 里。
 * 父加载器看不见子加载器的类，直接 Class.forName("net.minecraft.client.Minecraft")
 * 一定抛 ClassNotFoundException。必须先拿到真正的 ClassLoader。
 */
final class ClassLoaderUtil {

    private ClassLoaderUtil() {}

    /** 1.12.2 原版主类混淆名；Forge 下是 MCP 名 */
    private static final String[] MC_HINTS = {
            "net.minecraft.client.Minecraft",
            "bib",      // 1.12.2 vanilla
            "bhz",      // 1.12.1
            "bes",      // 1.12
    };

    static ClassLoader find(Instrumentation inst) {
        // 路径 A：Launch.classLoader（Forge / 带 LaunchWrapper 的原版）
        try {
            Class<?> launch = Class.forName("net.minecraft.launchwrapper.Launch");
            Field f = launch.getDeclaredField("classLoader");
            f.setAccessible(true);
            Object cl = f.get(null);
            if (cl instanceof ClassLoader) {
                return (ClassLoader) cl;
            }
        } catch (Throwable ignored) {
            // 走路径 B
        }

        // 路径 B：遍历已加载类（agentmain 才有 Instrumentation）
        if (inst != null) {
            for (Class<?> c : inst.getAllLoadedClasses()) {
                String n;
                try {
                    n = c.getName();
                } catch (Throwable t) {
                    continue;
                }
                for (String hint : MC_HINTS) {
                    if (hint.equals(n)) {
                        ClassLoader cl = c.getClassLoader();
                        if (cl != null) return cl;
                    }
                }
            }
        }

        // 路径 C：兜底，赌它在 system classpath 上
        return ClassLoader.getSystemClassLoader();
    }

    /** 在给定 ClassLoader 下按候选名找类，全失败返回 null */
    static Class<?> tryLoad(ClassLoader cl, String... names) {
        if (cl == null) return null;
        for (String n : names) {
            try {
                return Class.forName(n, false, cl);
            } catch (Throwable ignored) {
            }
        }
        return null;
    }
}
