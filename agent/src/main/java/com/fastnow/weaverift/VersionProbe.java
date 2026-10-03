package com.fastnow.weaverift;

import java.io.BufferedReader;
import java.lang.reflect.Field;
import java.lang.reflect.Method;
import java.net.URL;
import java.nio.file.Files;
import java.nio.file.Path;
import java.nio.file.Paths;
import java.security.CodeSource;
import java.util.regex.Matcher;
import java.util.regex.Pattern;

/**
 * 探测 (版本, 加载器, 名空间)。
 *
 * 名空间用"判别字段"自校验，而不是靠猜加载器。
 * 探测失败抛异常，不返回假版本号。
 *
 * 版本探测优先级：
 *   1. Minecraft 类的位置（jar 路径）—— 最可靠，跨加载器
 *   2. latest.log 里的版本行 —— 一定有
 *   3. Minecraft.getVersion() / SharedConstants / MCPVersion —— 兜底
 *   4. 显式传入的 version= —— 最高优先级（覆盖以上）
 */
public final class VersionProbe {

    public enum Namespace { SRG, OBF, MOJMAP, UNKNOWN }

    public static final class Result {
        public final String mcVersion;
        public final String loader;
        public final Namespace namespace;
        public final Class<?> minecraftClass;

        Result(String v, String l, Namespace n, Class<?> c) {
            mcVersion = v;
            loader = l;
            namespace = n;
            minecraftClass = c;
        }

        public String configFile() {
            return mcVersion + ".json";
        }

        @Override
        public String toString() {
            return mcVersion + "/" + loader + "/" + namespace;
        }
    }

    // ─────────────────── 主入口 ───────────────────

    public static Result probe(ClassLoader cl, String explicitVersion) throws Exception {
        String loader = detectLoader(cl);

        String version;
        if (explicitVersion != null && !explicitVersion.isEmpty()) {
            version = explicitVersion;
            System.out.println("[WeaveRift] 版本来源: 显式指定 (" + version + ")");
        } else {
            version = detectVersion(cl);
            if (version != null) {
                System.out.println("[WeaveRift] 版本来源: 自动探测 (" + version + ")");
            }
        }

        if (version == null || version.isEmpty()) {
            throw new IllegalStateException(
                    "无法确定 MC 版本，请用 version= 显式指定");
        }

        // 名空间探测
        Class<?> mc = null;
        for (String cand : new String[]{
                "net.minecraft.client.Minecraft",
                "bib",
                "bhz",
        }) {
            Class<?> c = ClassLoaderUtil.tryLoad(cl, cand);
            if (c != null) {
                mc = c;
                break;
            }
        }
        if (mc == null) {
            throw new IllegalStateException("找不到 Minecraft 主类");
        }

        Namespace ns;
        if (hasField(mc, "field_71439_g")) {
            ns = Namespace.SRG;
        } else if (hasField(mc, "h")) {
            ns = Namespace.OBF;
        } else if (hasField(mc, "player")) {
            ns = Namespace.MOJMAP;
        } else {
            ns = Namespace.UNKNOWN;
        }

        if (ns == Namespace.UNKNOWN) {
            throw new IllegalStateException(
                    "名空间探测失败，字段列表: "
                            + java.util.Arrays.toString(mc.getDeclaredFields()));
        }

        return new Result(version, loader, ns, mc);
    }

    // ─────────────────── 版本探测 ───────────────────

    private static String detectVersion(ClassLoader cl) {
        // 1. 从 Minecraft 类的位置推（最可靠）
        String v = versionFromClassLocation(cl);
        if (v != null) return v;

        // 2. latest.log
        v = versionFromLatestLog(cl);
        if (v != null) return v;

        // 3. Minecraft.getVersion()（老版本）
        for (String cls : new String[]{
                "net.minecraft.client.Minecraft",
                "bib",
                "bhz",
        }) {
            String ver = callStaticString(cl, cls, "getVersion");
            if (ver != null && !ver.isEmpty()) return ver;
        }

        // 4. SharedConstants.VERSION_STRING（1.14+）
        v = readStaticString(cl, "net.minecraft.SharedConstants", "VERSION_STRING");
        if (v != null) return v;

        // 5. MCPVersion.MCVERSION（Forge）
        v = readStaticString(cl,
                "net.minecraftforge.versions.mcp.MCPVersion", "MCVERSION");
        if (v != null) return v;

        return null;
    }

    // ─────────────────── 第 1 层：类位置 ───────────────────

    private static String versionFromClassLocation(ClassLoader cl) {
        for (String cls : new String[]{
                "net.minecraft.client.Minecraft",
                "bib",
                "bhz",
        }) {
            try {
                Class<?> c = ClassLoaderUtil.tryLoad(cl, cls);
                if (c == null) continue;

                CodeSource cs = c.getProtectionDomain().getCodeSource();
                if (cs == null || cs.getLocation() == null) continue;

                String path = cs.getLocation().getPath();
                String version = extractVersionFromPath(path);
                if (version != null) {
                    System.out.println(
                            "[WeaveRift] 版本来源: 类位置 " + path);
                    return version;
                }
            } catch (Throwable ignored) {
            }
        }
        return null;
    }

    private static String extractVersionFromPath(String path) {
        String raw = null;
        Matcher m = Pattern.compile("/versions/([^/]+)/").matcher(path);
        if (m.find()) {
            raw = m.group(1);
        }
        if (raw == null) {
            m = Pattern.compile("(?:forge|minecraft|neoforge|fabric)[-_](\\d+\\.\\d+(?:\\.\\d+)?)")
                .matcher(path);
            if (m.find()) return m.group(1);
        }
        if (raw == null) {
           m = Pattern.compile("/(\\d+\\.\\d+(?:\\.\\d+)?)\\.jar$").matcher(path);
            if (m.find()) return m.group(1);
        }
        return raw == null ? null : cleanVersion(raw);
    }
    
    private static String cleanVersion(String raw) {
        Matcher m = Pattern.compile("(\\d+\\.\\d+(?:\\.\\d+)?)").matcher(raw);
        if (m.find()) return m.group(1);
        return raw;
    }

    // ─────────────────── 第 2 层：latest.log ───────────────────

    private static String versionFromLatestLog(ClassLoader cl) {
        Path mcRoot = findMinecraftRoot(cl);
        if (mcRoot == null) return null;

        Path logPath = mcRoot.resolve("logs").resolve("latest.log");
        if (!Files.exists(logPath)) return null;

        try (BufferedReader r = Files.newBufferedReader(logPath)) {
            String line;
            int count = 0;
            while ((line = r.readLine()) != null && count++ < 200) {
                // 模式：Starting integrated minecraft server version 1.12.2
                Matcher m = Pattern
                        .compile("minecraft server version (\\S+)")
                        .matcher(line);
                if (m.find()) {
                    System.out.println("[WeaveRift] 版本来源: latest.log");
                    return m.group(1);
                }

                // 模式：Minecraft Version: 1.12.2（Forge 有）
                m = Pattern.compile("Minecraft Version: (\\S+)").matcher(line);
                if (m.find()) {
                    System.out.println("[WeaveRift] 版本来源: latest.log");
                    return m.group(1);
                }
            }
        } catch (Throwable ignored) {
        }
        return null;
    }

    /**
     * 从 Minecraft 类的位置推 .minecraft 根目录。
     *
     * 1.12.2：/versions/1.12.2/1.12.2.jar  → 上三级
     * 1.20.1：/versions/1.20.1/client.jar  → 上三级
     * Forge：/versions/1.12.2-forge-xxx/1.12.2-forge-xxx.jar  → 上三级
     */
    private static Path findMinecraftRoot(ClassLoader cl) {
        for (String cls : new String[]{
                "net.minecraft.client.Minecraft",
                "bib",
                "bhz",
        }) {
            try {
                Class<?> c = ClassLoaderUtil.tryLoad(cl, cls);
                if (c == null) continue;

                CodeSource cs = c.getProtectionDomain().getCodeSource();
                if (cs == null || cs.getLocation() == null) continue;

                URL url = cs.getLocation();
                Path jarPath;
                try {
                    jarPath = Paths.get(url.toURI());
                } catch (Throwable t) {
                    // URL 编码问题，退而用 getPath
                    jarPath = Paths.get(url.getPath());
                }

                // jarPath = /xxx/.minecraft/versions/1.12.2/1.12.2.jar
                // 上三级 = .minecraft
                Path parent = jarPath.getParent();          // versions/1.12.2
                if (parent == null) continue;
                parent = parent.getParent();                // versions
                if (parent == null) continue;
                parent = parent.getParent();                // .minecraft
                if (parent == null) continue;

                if (Files.exists(parent.resolve("logs"))) {
                    return parent;
                }
            } catch (Throwable ignored) {
            }
        }
        return null;
    }

    // ─────────────────── 第 3 层：静态方法/字段 ───────────────────

    private static String callStaticString(ClassLoader cl, String cls, String method) {
        try {
            Class<?> c = ClassLoaderUtil.tryLoad(cl, cls);
            if (c == null) return null;
            Method m = c.getDeclaredMethod(method);
            m.setAccessible(true);
            Object o = m.invoke(null);
            return o == null ? null : o.toString();
        } catch (Throwable t) {
            return null;
        }
    }

    private static String readStaticString(ClassLoader cl, String cls, String field) {
        try {
            Class<?> c = ClassLoaderUtil.tryLoad(cl, cls);
            if (c == null) return null;
            Field f = c.getDeclaredField(field);
            f.setAccessible(true);
            Object o = f.get(null);
            return o == null ? null : o.toString();
        } catch (Throwable t) {
            return null;
        }
    }

    // ─────────────────── 加载器探测 ───────────────────

    private static String detectLoader(ClassLoader cl) {
        if (ClassLoaderUtil.tryLoad(cl,
                "net.minecraftforge.common.MinecraftForge") != null) {
            return "forge";
        }
        if (ClassLoaderUtil.tryLoad(cl,
                "net.neoforged.neoforge.common.NeoForge") != null) {
            return "neoforge";
        }
        if (ClassLoaderUtil.tryLoad(cl,
                "net.fabricmc.loader.api.FabricLoader") != null) {
            return "fabric";
        }
        return "vanilla";
    }

    // ─────────────────── 名空间探测 ───────────────────

    private static boolean hasField(Class<?> c, String name) {
        try {
            c.getDeclaredField(name);
            return true;
        } catch (NoSuchFieldException e) {
            return false;
        } catch (Throwable t) {
            return false;
        }
    }

    private VersionProbe() {}
}