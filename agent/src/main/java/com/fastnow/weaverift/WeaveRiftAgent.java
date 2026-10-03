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
import java.util.List;
import java.util.Map;
import java.util.jar.JarFile;

public class WeaveRiftAgent {

    static final Map<String, String> CLASS_MAP = new HashMap<>();
    static final Map<String, String> FIELD_MAP = new HashMap<>();
    static final Map<String, MethodMapping> METHOD_MAP = new HashMap<>();

    static final class MethodMapping {
        final String obfName;
        final String obfSig;
        final String srgSig;

        MethodMapping(String obfName, String obfSig, String srgSig) {
            this.obfName = obfName;
            this.obfSig = obfSig;
            this.srgSig = srgSig;
        }
    }

    private static volatile String srgPath = "";
    private static volatile String version = "";
    private static volatile ClassLoader gameCl = null;
    private static volatile Instrumentation inst = null;
    private static volatile boolean ready = false;
    private static volatile String status = "init";
    private static volatile VersionProbe.Namespace namespace = VersionProbe.Namespace.UNKNOWN;

    private static Class<?> mcClass;
    private static Object mcInstance;
    private static Field playerField;
    private static Field xF, yF, zF, yawF, pitchF;
    private static Method healthM, maxHealthM;
    private static Field worldField, entityListField;

    private static Class<?> entityPlayerCls;
    private static Class<?> entityMobCls;
    private static Class<?> entityAnimalCls;

    private static Method mouseSetGrabbedM;
    private static Method displayIsActiveM;

    private static final int MAX_ENTS = 32;
    private static final long PERIOD_MS = 50;
    private static volatile boolean dumpedUnknown = false;

    public static void entry(String args) {
        System.out.println("[WeaveRift] entry, args=" + args);
        realInit(args, null);
    }

    public static void agentmain(String args, Instrumentation instrumentation) {
        System.out.println("[WeaveRift] agentmain, args=" + args);
        realInit(args, instrumentation);
    }

    public static void premain(String args, Instrumentation instrumentation) {
        agentmain(args, instrumentation);
    }

    private static void realInit(String args, Instrumentation instrumentation) {
        inst = instrumentation;
        parseArgs(args);

        if (instrumentation != null) {
            appendSelfToSystemClassLoader(instrumentation);
        } else {
            System.out.println("[WeaveRift] 无 Instrumentation（ClassLoader 模式）");
        }

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

    private static void parseArgs(String args) {
        if (args == null || args.isEmpty()) return;
        for (String part : args.split(";")) {
            if (part.startsWith("srg=")) {
                srgPath = part.substring(4).trim();
            } else if (part.startsWith("dll=")) {
                String dll = part.substring(4).trim();
                System.setProperty("weaverift.dll", dll);
                System.out.println("[WeaveRift] weaverift.dll = " + dll);
            } else if (part.startsWith("version=")) {
                version = part.substring(8).trim();
                System.out.println("[WeaveRift] version = " + version);
            }
        }
        if (srgPath.isEmpty()) srgPath = args.trim();
    }

    private static void appendSelfToSystemClassLoader(Instrumentation instrumentation) {
        if (instrumentation == null) return;
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

    private static void run() {
        try {
            status = "waiting";
            Thread.sleep(2000);

            reloadMappings();
            gameCl = ClassLoaderUtil.find(inst);
            System.out.println("[WeaveRift] game classloader = " + gameCl);

            VersionProbe.Result ver;
            try {
                ver = VersionProbe.probe(gameCl, version);
            } catch (Exception e) {
                System.out.println("[WeaveRift] 版本探测失败: " + e.getMessage());
                status = "version probe failed";
                RiftBridge.setError(status);
                return;
            }
            System.out.println("[WeaveRift] 探测结果: " + ver);
            namespace = ver.namespace;
            mcClass = ver.minecraftClass;
            System.out.println("[WeaveRift] Minecraft = " + mcClass.getName());

            VersionConfig config;
            try {
                config = VersionConfig.load(ver.mcVersion);
                System.out.println("[WeaveRift] VersionConfig: " + config);
            } catch (Throwable t) {
                System.out.println("[WeaveRift] VersionConfig 加载失败: " + t);
                status = "config load failed";
                RiftBridge.setError(status);
                return;
            }

            NamespaceResolver resolver = new NamespaceResolver(
                    namespace, CLASS_MAP, FIELD_MAP, METHOD_MAP);

            ClassCache.init(gameCl, config, resolver);
            FieldCache.init(config, resolver);
            MethodCache.init(config, resolver);

            mcInstance = MethodCache.invokeStatic("Minecraft.func_71410_x");
            if (mcInstance == null) {
                status = "mc instance null";
                RiftBridge.setError(status);
                return;
            }

            playerField = FieldCache.get("Minecraft.field_71439_g");
            xF = FieldCache.get("Entity.field_70165_t");
            yF = FieldCache.get("Entity.field_70163_u");
            zF = FieldCache.get("Entity.field_70161_v");
            yawF = FieldCache.get("Entity.field_70177_z");
            pitchF = FieldCache.get("Entity.field_70125_A");
            healthM = MethodCache.get("EntityLivingBase.func_110143_aJ");
            maxHealthM = MethodCache.get("EntityLivingBase.func_110138_aP");

            worldField = FieldCache.get("Minecraft.field_71441_e");
            entityListField = FieldCache.get("World.field_72996_f");

            if (playerField == null) {
                status = "playerField null";
                RiftBridge.setError(status);
                return;
            }

            initEntityClasses();
            initInputBridge();

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

    private static void initEntityClasses() {
        entityPlayerCls = loadBySrg("net/minecraft/entity/player/EntityPlayer");
        entityMobCls = loadBySrg("net/minecraft/entity/monster/IMob");
        entityAnimalCls = loadBySrg("net/minecraft/entity/passive/EntityAnimal");
        System.out.println("[WeaveRift] entity classes: player=" + entityPlayerCls
                + " mob=" + entityMobCls + " animal=" + entityAnimalCls);
    }

    private static void initInputBridge() {
        try {
            Class<?> mouseCls = Class.forName("org.lwjgl.input.Mouse", false, gameCl);
            mouseSetGrabbedM = mouseCls.getMethod("setGrabbed", boolean.class);
            System.out.println("[WeaveRift] Mouse.setGrabbed 已绑定");
        } catch (Throwable t) {
            System.out.println("[WeaveRift] Mouse.setGrabbed 绑定失败: " + t);
        }

        try {
            Class<?> displayCls = Class.forName("org.lwjgl.opengl.Display", false, gameCl);
            displayIsActiveM = displayCls.getMethod("isActive");
            System.out.println("[WeaveRift] Display.isActive 已绑定");
        } catch (Throwable t) {
            System.out.println("[WeaveRift] Display.isActive 绑定失败: " + t);
        }
    }

    private static Class<?> loadBySrg(String srgFull) {
        String obf = CLASS_MAP.get(srgFull);
        String name = obf != null ? obf.replace('/', '.') : srgFull.replace('/', '.');
        try {
            return Class.forName(name, false, gameCl);
        } catch (Throwable t) {
            System.out.println("[WeaveRift] loadBySrg 失败: " + srgFull
                    + " (obf=" + obf + ") - " + t);
            return null;
        }
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
            if (healthM != null) hp = ((Number) healthM.invoke(p)).floatValue();
            if (maxHealthM != null) maxHp = ((Number) maxHealthM.invoke(p)).floatValue();

            List<?> ents = null;
            try {
                if (worldField != null && entityListField != null) {
                    Object w = worldField.get(mcInstance);
                    if (w != null) ents = (List<?>) entityListField.get(w);
                }
            } catch (Throwable ignored) {}

            int entCount = ents == null ? -1 : ents.size();

            double[] out = new double[9 + MAX_ENTS * 6];
            out[0] = 1;
            out[1] = x;
            out[2] = y;
            out[3] = z;
            out[4] = yaw;
            out[5] = pitch;
            out[6] = hp;
            out[7] = maxHp;
            out[8] = entCount;

            int n = 0;
            if (ents != null) {
                for (Object e : ents) {
                    if (n >= MAX_ENTS) break;
                    if (e == p) continue;
                    try {
                        Class<?> ec = e.getClass();
                        double ex = getDouble(e, ec, xF.getName());
                        double ey = getDouble(e, ec, yF.getName());
                        double ez = getDouble(e, ec, zF.getName());
                        float ehp = 20f;
                        if (healthM != null) {
                            try { ehp = ((Number) healthM.invoke(e)).floatValue(); }
                            catch (Throwable ignored) {}
                        }
                        int type = classifyEntity(ec);

                        if (type == 0 && !dumpedUnknown) {
                            dumpedUnknown = true;
                            System.out.println("[WeaveRift] unknown entity: " + ec.getName());
                        }

                        int off = 9 + n * 6;
                        out[off]     = ex;
                        out[off + 1] = ey;
                        out[off + 2] = ez;
                        out[off + 3] = ehp;
                        out[off + 4] = type;
                        out[off + 5] = 0;
                        n++;
                    } catch (Throwable ignored) {}
                }
            }

            RiftBridge.updateSnapshotFull(out);

            updateInputState();
        } catch (Throwable t) {
            RiftBridge.updateSnapshot(0, 0, 0, 0, 0, 0, 0, 0, -1);
            RiftBridge.setError(String.valueOf(t));
        }
    }

    private static void updateInputState() {
        try {
            if (mouseSetGrabbedM != null) {
                mouseSetGrabbedM.invoke(null, RiftBridge.isMouseGrabbed());
            }
        } catch (Throwable ignored) {}

        try {
            if (displayIsActiveM != null) {
                displayIsActiveM.invoke(null);
            }
        } catch (Throwable ignored) {}
    }

    private static double getDouble(Object obj, Class<?> cls, String fieldName) {
        Class<?> c = cls;
        while (c != null) {
            try {
                Field f = c.getDeclaredField(fieldName);
                f.setAccessible(true);
                return f.getDouble(obj);
            } catch (Throwable ignored) {}
            c = c.getSuperclass();
        }
        return 0;
    }

    private static int classifyEntity(Class<?> ec) {
        if (entityPlayerCls != null && entityPlayerCls.isAssignableFrom(ec)) return 1;
        if (entityMobCls != null && entityMobCls.isAssignableFrom(ec)) return 2;
        if (entityAnimalCls != null && entityAnimalCls.isAssignableFrom(ec)) return 3;
        return 0;
    }

    static boolean reloadMappings() {
        CLASS_MAP.clear();
        FIELD_MAP.clear();
        METHOD_MAP.clear();
        try {
            if (srgPath.isEmpty() || !Files.exists(Paths.get(srgPath))) {
                status = "srg missing (走纯 SRG 名路径)";
                System.out.println("[WeaveRift] 未配置 srg，假定运行时已是 SRG 名（Forge）");
                return true;
            }
            try (BufferedReader r = new BufferedReader(new FileReader(srgPath))) {
                String line;
                while ((line = r.readLine()) != null) {
                    line = line.trim();
                    if (line.isEmpty() || line.startsWith("#")) continue;
                    if (line.startsWith("CL:")) {
                        String[] p = line.substring(3).trim().split("\\s+");
                        if (p.length >= 2) CLASS_MAP.put(p[1], p[0]);
                    } else if (line.startsWith("FD:")) {
                        String[] p = line.substring(3).trim().split("\\s+");
                        if (p.length >= 2) FIELD_MAP.put(p[1], p[0]);
                    } else if (line.startsWith("MD:")) {
                        String[] p = line.substring(3).trim().split("\\s+");
                        if (p.length >= 4) {
                            String obfPath = p[0];
                            String obfSig = p[1];
                            String srgPath = p[2];
                            String srgSig = p[3];
                            String obfName = obfPath.substring(obfPath.lastIndexOf('/') + 1);
                            METHOD_MAP.put(srgPath, new MethodMapping(obfName, obfSig, srgSig));
                        }
                    }
                }
            }
            status = "mappings ok (" + CLASS_MAP.size() + " classes)";
            System.out.println("[WeaveRift] SRG 载入: " + srgPath
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
}