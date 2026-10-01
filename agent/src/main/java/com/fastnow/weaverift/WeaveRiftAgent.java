package com.fastnow.weaverift;

import java.io.BufferedReader;
import java.io.File;
import java.io.FileReader;
import java.lang.instrument.Instrumentation;
import java.lang.reflect.Field;
import java.lang.reflect.Method;
import java.nio.file.Files;
import java.nio.file.Paths;
import java.util.HashMap;
import java.util.Map;
import java.util.jar.JarFile;

/**
 * WeaveRift agent —— 只读采样，不改任何字节码。
 *
 * <p>v3 起支持两种加载方式：
 * <ul>
 *   <li><b>Attach API</b>：agentmain(args, instrumentation)，instrumentation 非 null</li>
 *   <li><b>ClassLoader 直接调用</b>：agentmain(args, null)，instrumentation 为 null</li>
 * </ul>
 *
 * <p>第二种方式完全绕过 jdk.attach.allowAttachSelf 限制。
 */
public class WeaveRiftAgent {

    private static final Map<String, String> CLASS_MAP = new HashMap<>();
    private static final Map<String, String> FIELD_MAP = new HashMap<>();
    private static final Map<String, String> METHOD_MAP = new HashMap<>();

    private static volatile String srgPath = "";
    private static volatile ClassLoader gameCl = null;
    private static volatile Instrumentation inst = null;
    private static volatile boolean ready = false;
    private static volatile String status = "init";

    private static Class<?> mcClass;
    private static Object mcInstance;
    private static Field playerField;
    private static Field xF, yF, zF, yawF, pitchF;
    private static Method healthM, maxHealthM;
    private static Field worldField, entityListField;

    private static final long PERIOD_MS = 50;

    // ─────────────────────── 入口 ───────────────────────

    public static void agentmain(String args, Instrumentation instrumentation) {
        System.out.println("[WeaveRift] agentmain, args=" + args);
        inst = instrumentation;
        parseArgs(args);

        // ★ instrumentation 为 null 时（ClassLoader 模式），跳过 append
        if (instrumentation != null) {
            appendSelfToSystemClassLoader(instrumentation);
        } else {
            System.out.println("[WeaveRift] 无 Instrumentation（ClassLoader 模式）");
        }

        // 反向注册 RiftBridge
        try {
            NativeBridge.registerBridge(RiftBridge.class);
            System.out.println("[WeaveRift] RiftBridge 注册成功");
        } catch (Throwable t) {
            System.out.println("[WeaveRift] RiftBridge 注册失败: " + t);
            t.printStackTrace();
        }

        Thread t = new Thread(WeaveRiftAgent::run, "WeaveRift-Sampler");
        t.setDaemon(true);
        t.start();
    }

    public static void premain(String args, Instrumentation instrumentation) {
        agentmain(args, instrumentation);
    }

    private static void parseArgs(String args) {
        if (args == null || args.isEmpty()) {
            return;
        }
        for (String part : args.split(";")) {
            if (part.startsWith("srg=")) {
                srgPath = part.substring(4).trim();
            } else if (part.startsWith("dll=")) {
                String dll = part.substring(4).trim();
                System.setProperty("weaverift.dll", dll);
                System.out.println("[WeaveRift] weaverift.dll = " + dll);
            }
        }
        if (srgPath.isEmpty()) {
            srgPath = args.trim();
        }
    }

    private static void appendSelfToSystemClassLoader(Instrumentation instrumentation) {
        if (instrumentation == null) {
            return;
        }
        try {
            File self = new File(WeaveRiftAgent.class.getProtectionDomain()
                    .getCodeSource().getLocation().toURI());
            if (self.isFile()) {
                instrumentation.appendToSystemClassLoaderSearch(new JarFile(self));
                System.out.println("[WeaveRift] appended to system CL: " + self);
            }
        } catch (Throwable t) {
            System.out.println("[WeaveRift] appendToSystemClassLoaderSearch 失败: " + t);
        }
    }

    // ─────────────────────── 主循环 ───────────────────────

    private static void run() {
        try {
            status = "waiting";
            Thread.sleep(2000);

            reloadMappings();
            gameCl = ClassLoaderUtil.find(inst);
            System.out.println("[WeaveRift] game classloader = " + gameCl);

            mcClass = ClassLoaderUtil.tryLoad(gameCl,
                    "net.minecraft.client.Minecraft",
                    obfClassOf("net/minecraft/client/Minecraft"));
            if (mcClass == null) {
                status = "mc class not found";
                RiftBridge.setError(status);
                System.out.println("[WeaveRift] 找不到 Minecraft 类");
                return;
            }
            System.out.println("[WeaveRift] Minecraft = " + mcClass.getName());

            mcInstance = invokeStatic(mcClass,
                    "net/minecraft/client/Minecraft/func_71410_x");
            if (mcInstance == null) {
                status = "mc instance null";
                RiftBridge.setError(status);
                return;
            }

            playerField = findField(mcClass,
                    "net/minecraft/client/Minecraft/field_71439_g");
            xF = findField(playerClass(), "net/minecraft/entity/Entity/field_70165_t");
            yF = findField(playerClass(), "net/minecraft/entity/Entity/field_70163_u");
            zF = findField(playerClass(), "net/minecraft/entity/Entity/field_70161_v");
            yawF = findField(playerClass(), "net/minecraft/entity/Entity/field_70177_z");
            pitchF = findField(playerClass(), "net/minecraft/entity/Entity/field_70125_A");
            healthM = findMethod(playerClass(),
                    "net/minecraft/entity/EntityLivingBase/func_110143_aJ");
            maxHealthM = findMethod(playerClass(),
                    "net/minecraft/entity/EntityLivingBase/func_110138_aP");

            worldField = findField(mcClass,
                    "net/minecraft/client/Minecraft/field_71441_e");
            Class<?> worldCl = ClassLoaderUtil.tryLoad(gameCl,
                    "net.minecraft.world.World",
                    obfClassOf("net/minecraft/world/World"));
            if (worldCl != null) {
                entityListField = findField(worldCl,
                        "net/minecraft/world/World/field_72996_f");
            }

            ready = true;
            status = "ready";
            System.out.println("[WeaveRift] 采样就绪");

            while (true) {
                sample();
                Thread.sleep(PERIOD_MS);
            }
        } catch (Throwable t) {
            status = "error: " + t;
            RiftBridge.setError(status);
            System.out.println("[WeaveRift] 采样线程异常: " + t);
            t.printStackTrace();
        }
    }

    private static Class<?> playerClass() {
        try {
            Object p = playerField.get(mcInstance);
            if (p != null) {
                return p.getClass();
            }
        } catch (Throwable ignored) {
        }
        return ClassLoaderUtil.tryLoad(gameCl,
                "net.minecraft.client.entity.EntityPlayerSP",
                "net.minecraft.entity.player.EntityPlayer",
                "net.minecraft.entity.EntityLivingBase",
                "net.minecraft.entity.Entity");
    }

    private static void sample() {
        try {
            Object p = playerField.get(mcInstance);
            if (p == null) {
                RiftBridge.updateSnapshot(0, 0, 0, 0, 0, 0, 0, 0, -1);
                return;
            }
            double x = xF.getDouble(p);
            double y = yF.getDouble(p);
            double z = zF.getDouble(p);
            float yaw = yawF.getFloat(p);
            float pitch = pitchF.getFloat(p);

            float hp = 20f, maxHp = 20f;
            if (healthM != null) {
                hp = ((Number) healthM.invoke(p)).floatValue();
            }
            if (maxHealthM != null) {
                maxHp = ((Number) maxHealthM.invoke(p)).floatValue();
            }

            int ents = -1;
            try {
                if (worldField != null && entityListField != null) {
                    Object w = worldField.get(mcInstance);
                    if (w != null) {
                        ents = ((java.util.List<?>) entityListField.get(w)).size();
                    }
                }
            } catch (Throwable ignored) {
            }

            RiftBridge.updateSnapshot(1, x, y, z, yaw, pitch, hp, maxHp, ents);
        } catch (Throwable t) {
            RiftBridge.updateSnapshot(0, 0, 0, 0, 0, 0, 0, 0, -1);
            RiftBridge.setError(String.valueOf(t));
        }
    }

    // ─────────────────────── 反射工具 ───────────────────────

    private static Field findField(Class<?> cls, String mappedFull) {
        if (cls == null || mappedFull == null) {
            return null;
        }
        String srg = simple(mappedFull);
        String obf = FIELD_MAP.get(mappedFull);
        String obfSimple = obf == null ? null : simple(obf);

        Class<?> c = cls;
        while (c != null) {
            for (String n : new String[]{srg, obfSimple}) {
                if (n == null) {
                    continue;
                }
                try {
                    Field f = c.getDeclaredField(n);
                    f.setAccessible(true);
                    return f;
                } catch (Throwable ignored) {
                }
            }
            c = c.getSuperclass();
        }
        log("[WeaveRift] 字段缺失: " + mappedFull);
        return null;
    }

    private static Method findMethod(Class<?> cls, String mappedFull) {
        if (cls == null || mappedFull == null) {
            return null;
        }
        String srg = simple(mappedFull);
        String obf = METHOD_MAP.get(mappedFull);
        String obfSimple = obf == null ? null : simple(obf);

        Class<?> c = cls;
        while (c != null) {
            for (String n : new String[]{srg, obfSimple}) {
                if (n == null) {
                    continue;
                }
                try {
                    Method m = c.getDeclaredMethod(n);
                    m.setAccessible(true);
                    return m;
                } catch (Throwable ignored) {
                }
            }
            c = c.getSuperclass();
        }
        log("[WeaveRift] 方法缺失: " + mappedFull);
        return null;
    }

    private static Object invokeStatic(Class<?> cls, String mappedFull) {
        String srg = simple(mappedFull);
        String obf = METHOD_MAP.get(mappedFull);
        String obfSimple = obf == null ? null : simple(obf);
        for (String n : new String[]{srg, obfSimple}) {
            if (n == null) {
                continue;
            }
            try {
                Method m = cls.getDeclaredMethod(n);
                m.setAccessible(true);
                return m.invoke(null);
            } catch (Throwable ignored) {
            }
        }
        log("[WeaveRift] 静态方法缺失: " + mappedFull);
        return null;
    }

    private static String obfClassOf(String mapped) {
        String obf = CLASS_MAP.get(mapped);
        return obf == null ? null : obf.replace('/', '.');
    }

    private static String simple(String full) {
        int i = full.lastIndexOf('/');
        return i < 0 ? full : full.substring(i + 1);
    }

    // ─────────────────────── SRG 映射 ───────────────────────

    static boolean reloadMappings() {
        CLASS_MAP.clear();
        FIELD_MAP.clear();
        METHOD_MAP.clear();
        try {
            if (srgPath.isEmpty() || !Files.exists(Paths.get(srgPath))) {
                status = "srg missing (走纯 SRG 名路径)";
                log("[WeaveRift] 未配置 srg，假定运行时已是 SRG 名（Forge）");
                return true;
            }
            try (BufferedReader r = new BufferedReader(new FileReader(srgPath))) {
                String line;
                while ((line = r.readLine()) != null) {
                    line = line.trim();
                    if (line.isEmpty() || line.startsWith("#")) {
                        continue;
                    }
                    if (line.startsWith("CL:")) {
                        String[] p = line.substring(3).trim().split("\\s+");
                        if (p.length >= 2) {
                            CLASS_MAP.put(p[1], p[0]);
                        }
                    } else if (line.startsWith("FD:")) {
                        String[] p = line.substring(3).trim().split("\\s+");
                        if (p.length >= 2) {
                            FIELD_MAP.put(p[1], p[0]);
                        }
                    } else if (line.startsWith("MD:")) {
                        String[] p = line.substring(3).trim().split("\\s+");
                        if (p.length >= 4) {
                            METHOD_MAP.put(p[2], p[0]);
                        }
                    }
                }
            }
            status = "mappings ok (" + CLASS_MAP.size() + " classes)";
            log("[WeaveRift] SRG 载入: " + srgPath
                    + " class=" + CLASS_MAP.size()
                    + " field=" + FIELD_MAP.size()
                    + " method=" + METHOD_MAP.size());
            return true;
        } catch (Throwable t) {
            status = "srg error: " + t;
            RiftBridge.setError(status);
            return false;
        }
    }

    static String describe() {
        return "ready=" + ready + " status=" + status
                + " cl=" + (gameCl == null ? "null" : gameCl.getClass().getName())
                + " mc=" + (mcClass == null ? "null" : mcClass.getName());
    }

    private static void log(String s) {
        System.out.println(s);
    }
}