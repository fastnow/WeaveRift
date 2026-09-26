package com.fastnow.weaverift;

import org.objectweb.asm.ClassReader;
import org.objectweb.asm.ClassVisitor;
import org.objectweb.asm.ClassWriter;
import org.objectweb.asm.MethodVisitor;
import org.objectweb.asm.Opcodes;

import java.io.BufferedReader;
import java.io.FileReader;
import java.io.InputStream;
import java.lang.instrument.Instrumentation;
import java.lang.reflect.Field;
import java.lang.reflect.Method;
import java.nio.file.Files;
import java.nio.file.Paths;
import java.util.HashMap;
import java.util.Map;

public class WeaveRiftAgent {

    private static Instrumentation instrumentation;

    private static final Map<String, String> CLASS_MAP = new HashMap<>();
    private static final Map<String, String> FIELD_MAP = new HashMap<>();
    private static final Map<String, String> METHOD_MAP = new HashMap<>();

    private static String srgPath = null;

    public static void premain(String args, Instrumentation inst) {
        init(inst, args);
    }

    public static void agentmain(String args, Instrumentation inst) {
        init(inst, args);
    }

    private static void init(Instrumentation inst, String args) {
        instrumentation = inst;
        System.out.println("[WeaveRift] Agent loaded, inst=" + (inst != null));
        new Thread(WeaveRiftAgent::mainLoop, "WeaveRift-Main").start();
    }

    private static void mainLoop() {
        try {
            // 等 weaverift.srg 属性就绪
            String srgProp = null;
            for (int i = 0; i < 100; i++) {
                srgProp = System.getProperty("weaverift.srg");
                if (srgProp != null && !srgProp.isEmpty()) break;
                Thread.sleep(100);
            }

            System.out.println("[WeaveRift] Starting probe...");

            if (!loadSrgMappings()) {
                System.out.println("[WeaveRift] ERROR: Cannot load SRG mappings!");
                return;
            }
            System.out.println("[WeaveRift] Loaded " + CLASS_MAP.size() + " classes, "
                    + FIELD_MAP.size() + " fields, " + METHOD_MAP.size() + " methods");

            // ─── 解析 Minecraft 类名 ──────────────────────────
            String mcClassName = CLASS_MAP.get("net/minecraft/client/Minecraft");
            if (mcClassName == null) {
                System.out.println("[WeaveRift] ERROR: Minecraft class not in SRG map!");
                return;
            }
            mcClassName = mcClassName.replace('/', '.');
            String mcClassInternal = mcClassName.replace('.', '/');

            String renderObf = METHOD_MAP.get("net/minecraft/client/Minecraft/func_71411_J");
            String renderMethodName = renderObf != null
                    ? renderObf.substring(renderObf.lastIndexOf('/') + 1)
                    : "func_71411_J";

            System.out.println("[WeaveRift] Minecraft class: " + mcClassName);
            System.out.println("[WeaveRift] render method: " + renderMethodName);

            // ─── 读取原始字节码 → ASM 修改 → JVMTI 替换 ────
            try {
                patchAndRedefine(mcClassInternal, renderMethodName);
            } catch (Throwable t) {
                System.out.println("[WeaveRift] Failed to patch render method:");
                t.printStackTrace();
            }

            // ─── 反射读玩家数据 ──────────────────────────────
            Class<?> mcClass = Class.forName(mcClassName);
            Object mc = getMinecraftInstance(mcClass);
            if (mc == null) {
                System.out.println("[WeaveRift] ERROR: Minecraft instance is null!");
                return;
            }
            System.out.println("[WeaveRift] Minecraft instance: " + mc);

            String playerFieldName = getObfFieldName(
                    "net/minecraft/client/Minecraft/field_71439_g",
                    "net/minecraft/client/Minecraft/thePlayer"
            );
            if (playerFieldName == null) {
                System.out.println("[WeaveRift] ERROR: player field not in SRG map!");
                return;
            }
            System.out.println("[WeaveRift] player field name: " + playerFieldName);

            Field playerField = mcClass.getDeclaredField(playerFieldName);
            playerField.setAccessible(true);

            while (true) {
                Object player = playerField.get(mc);
                if (player != null) {
                    Class<?> playerClass = player.getClass();
                    double x = getDoubleField(player, playerClass, "field_70165_t");
                    double y = getDoubleField(player, playerClass, "field_70163_u");
                    double z = getDoubleField(player, playerClass, "field_70161_v");
                    System.out.printf("[WeaveRift] Player pos: %.2f %.2f %.2f%n", x, y, z);
                }
                Thread.sleep(1000);
            }

        } catch (InterruptedException e) {
            Thread.currentThread().interrupt();
        } catch (Exception e) {
            System.out.println("[WeaveRift] Fatal error:");
            e.printStackTrace();
        }
    }

    /**
     * 读原始字节码 → ASM 在 render 方法末尾插入 RiftRender.onRender() → 调 NativeBridge 替换
     */
    private static void patchAndRedefine(String classInternal, String renderMethodName) throws Exception {
        String resourcePath = classInternal + ".class";

        // 1. 从 classpath 读原始字节码
        ClassLoader cl = Thread.currentThread().getContextClassLoader();
        if (cl == null) cl = ClassLoader.getSystemClassLoader();

        InputStream is = cl.getResourceAsStream(resourcePath);
        if (is == null) {
            System.out.println("[WeaveRift] Cannot read resource: " + resourcePath);
            return;
        }
        byte[] originalBytes = is.readAllBytes();
        is.close();
        System.out.println("[WeaveRift] Read " + originalBytes.length + " bytes for " + resourcePath);

        // 2. ASM 修改
        ClassReader reader = new ClassReader(originalBytes);
        ClassWriter writer = new ClassWriter(reader, ClassWriter.COMPUTE_FRAMES);

        ClassVisitor visitor = new ClassVisitor(Opcodes.ASM9, writer) {
            @Override
            public MethodVisitor visitMethod(int access, String name, String desc,
                                             String signature, String[] exceptions) {
                MethodVisitor mv = super.visitMethod(access, name, desc, signature, exceptions);

                if (!name.equals(renderMethodName)) return mv;

                System.out.println("[WeaveRift] Patching method: " + name + " " + desc);

                return new MethodVisitor(Opcodes.ASM9, mv) {
                    @Override
                    public void visitInsn(int opcode) {
                        if (opcode == Opcodes.RETURN) {
                            mv.visitMethodInsn(
                                    Opcodes.INVOKESTATIC,
                                    "com/fastnow/weaverift/render/RiftRender",
                                    "onRender",
                                    "()V",
                                    false
                            );
                        }
                        super.visitInsn(opcode);
                    }
                };
            }
        };

        reader.accept(visitor, ClassReader.EXPAND_FRAMES);
        byte[] patchedBytes = writer.toByteArray();
        System.out.println("[WeaveRift] Patched: " + originalBytes.length + " -> " + patchedBytes.length);

        // 3. 调 NativeBridge 用 JVMTI 替换
        int rc = NativeBridge.redefineClass(classInternal.replace('/', '.'), patchedBytes);
        if (rc == 0) {
            System.out.println("[WeaveRift] redefineClass succeeded!");
        } else {
            System.out.println("[WeaveRift] redefineClass failed: " + rc);
        }
    }

    private static boolean loadSrgMappings() {
        srgPath = System.getProperty("weaverift.srg");
        if (srgPath == null || srgPath.isEmpty()) {
            System.out.println("[WeaveRift] SRG path not set in system properties");
            return false;
        }
        java.nio.file.Path p = Paths.get(srgPath);
        if (!Files.exists(p)) {
            System.out.println("[WeaveRift] SRG file not found: " + srgPath);
            return false;
        }
        System.out.println("[WeaveRift] Loading SRG from: " + srgPath);

        try (BufferedReader reader = new BufferedReader(new FileReader(srgPath))) {
            String line;
            while ((line = reader.readLine()) != null) {
                line = line.trim();
                if (line.isEmpty() || line.startsWith("#")) continue;

                if (line.startsWith("CL:")) {
                    String[] parts = line.substring(3).trim().split("\\s+");
                    if (parts.length >= 2) CLASS_MAP.put(parts[1], parts[0]);
                } else if (line.startsWith("FD:")) {
                    String[] parts = line.substring(3).trim().split("\\s+");
                    if (parts.length >= 2) FIELD_MAP.put(parts[1], parts[0]);
                } else if (line.startsWith("MD:")) {
                    String[] parts = line.substring(3).trim().split("\\s+");
                    if (parts.length >= 4) METHOD_MAP.put(parts[2], parts[0]);
                }
            }
            return true;
        } catch (Exception e) {
            System.out.println("[WeaveRift] SRG parse error: " + e.getMessage());
            return false;
        }
    }

    private static Object getMinecraftInstance(Class<?> mcClass) {
        String methodObf = METHOD_MAP.get("net/minecraft/client/Minecraft/func_71410_x");
        if (methodObf != null) {
            String methodName = methodObf.substring(methodObf.lastIndexOf('/') + 1);
            try {
                Method m = mcClass.getMethod(methodName);
                Object result = m.invoke(null);
                if (result != null) {
                    System.out.println("[WeaveRift] Got Minecraft via map: " + methodName);
                    return result;
                }
            } catch (Exception e) {
                System.out.println("[WeaveRift] getMinecraft via map failed: " + e.getMessage());
            }
        }
        for (Method m : mcClass.getMethods()) {
            if (!java.lang.reflect.Modifier.isStatic(m.getModifiers())) continue;
            if (m.getParameterCount() != 0) continue;
            if (m.getReturnType() != mcClass) continue;
            try {
                Object result = m.invoke(null);
                if (result != null) {
                    System.out.println("[WeaveRift] Got Minecraft via scan: " + m.getName());
                    return result;
                }
            } catch (Exception ignored) {}
        }
        return null;
    }

    private static String getObfFieldName(String... standardNames) {
        for (String std : standardNames) {
            String obf = FIELD_MAP.get(std);
            if (obf != null) return obf.substring(obf.lastIndexOf('/') + 1);
        }
        return null;
    }

    private static double getDoubleField(Object obj, Class<?> cls, String srgName) {
        String fieldName = null;
        String fieldObf = FIELD_MAP.get("net/minecraft/entity/Entity/" + srgName);
        if (fieldObf == null) fieldObf = FIELD_MAP.get("net/minecraft/entity/player/EntityPlayer/" + srgName);
        if (fieldObf == null) fieldObf = FIELD_MAP.get("net/minecraft/client/entity/EntityPlayerSP/" + srgName);
        if (fieldObf != null) fieldName = fieldObf.substring(fieldObf.lastIndexOf('/') + 1);
        if (fieldName == null) fieldName = srgName;

        Class<?> current = cls;
        while (current != null) {
            try {
                Field f = current.getDeclaredField(fieldName);
                f.setAccessible(true);
                return f.getDouble(obj);
            } catch (Exception ignored) {}
            current = current.getSuperclass();
        }
        return 0.0;
    }

    public static Instrumentation getInstrumentation() {
        return instrumentation;
    }
}