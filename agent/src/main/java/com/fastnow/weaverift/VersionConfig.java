package com.fastnow.weaverift;

import java.io.BufferedReader;
import java.io.InputStream;
import java.io.InputStreamReader;
import java.nio.charset.StandardCharsets;
import java.util.HashMap;
import java.util.Map;

/**
 * 版本配置：从 jar 内 versions/&lt;mc_version&gt;.json 读取。
 *
 * <p>手动解析 JSON（项目无 gson/jackson 依赖）。
 *
 * <p>支持的结构（本项目的 JSON 只用到这几种）：
 * <pre>
 * {
 *   "key": "value",                       // 字符串值
 *   "probe": { "k": "v" },                // flat object
 *   "classes": { "k": { "k2": "v2" } }    // object of objects
 * }
 * </pre>
 *
 * <p>不支持：数组、数字（除 schema_version）、布尔、null。
 * schema_version 是数字，但我们也当字符串读（JSON 里可以写成 "2" 或 2）。
 */
public final class VersionConfig {

    // ─── 数据结构 ───

    public static final class ClassDef {
        public final String srg;
        public final String obf;   // 可选，探测入口才有

        ClassDef(String srg, String obf) {
            this.srg = srg;
            this.obf = obf;
        }

        @Override
        public String toString() {
            return "ClassDef{srg=" + srg + ", obf=" + obf + "}";
        }
    }

    public static final class FieldDef {
        public final String owner;
        public final String srg;
        public final String type;

        FieldDef(String owner, String srg, String type) {
            this.owner = owner;
            this.srg = srg;
            this.type = type;
        }

        @Override
        public String toString() {
            return "FieldDef{owner=" + owner + ", srg=" + srg + ", type=" + type + "}";
        }
    }

    public static final class MethodDef {
        public final String owner;
        public final String srg;
        public final String sig;

        MethodDef(String owner, String srg, String sig) {
            this.owner = owner;
            this.srg = srg;
            this.sig = sig;
        }

        @Override
        public String toString() {
            return "MethodDef{owner=" + owner + ", srg=" + srg + ", sig=" + sig + "}";
        }
    }

    // ─── 字段 ───

    private final int schemaVersion;
    private final String mcVersion;
    private final String namespace;

    private final Map<String, ClassDef> classes = new HashMap<>();
    private final Map<String, FieldDef> fields = new HashMap<>();
    private final Map<String, MethodDef> methods = new HashMap<>();

    private final String probeClass;
    private final String probeDiscriminator;

    // ─── 访问器 ───

    public int schemaVersion() { return schemaVersion; }
    public String mcVersion() { return mcVersion; }
    public String namespace() { return namespace; }
    public Map<String, ClassDef> classes() { return classes; }
    public Map<String, FieldDef> fields() { return fields; }
    public Map<String, MethodDef> methods() { return methods; }
    public String probeClass() { return probeClass; }
    public String probeDiscriminator() { return probeDiscriminator; }

    // ─── 加载 ───

    public static VersionConfig load(String mcVersion) throws Exception {
        String path = "/versions/" + mcVersion + ".json";
        try (InputStream is = VersionConfig.class.getResourceAsStream(path)) {
            if (is == null) {
                throw new IllegalStateException(
                        "找不到版本配置: " + path
                                + "（检查 agent jar 是否打包了 resources/versions/）");
            }
            String json = readAll(is);
            return parse(json);
        }
    }

    private static String readAll(InputStream is) throws Exception {
        StringBuilder sb = new StringBuilder();
        try (BufferedReader r = new BufferedReader(
                new InputStreamReader(is, StandardCharsets.UTF_8))) {
            char[] buf = new char[4096];
            int n;
            while ((n = r.read(buf)) > 0) {
                sb.append(buf, 0, n);
            }
        }
        return sb.toString();
    }

    // ─── 解析 ───

    private static VersionConfig parse(String json) throws Exception {
        Parser p = new Parser(json);

        int schemaVersion = 0;
        String mcVersion = "";
        String namespace = "";
        String probeClass = "";
        String probeDiscriminator = "";

        Map<String, ClassDef> classes = new HashMap<>();
        Map<String, FieldDef> fields = new HashMap<>();
        Map<String, MethodDef> methods = new HashMap<>();

        p.expect('{');
        while (true) {
            p.skipWs();
            if (p.peek() == '}') {
                p.next();
                break;
            }

            String key = p.readString();
            p.expect(':');
            p.skipWs();

            if (p.peek() == '{') {
                // 嵌套 object
                if ("probe".equals(key)) {
                    Map<String, String> probeMap = p.readFlatObject();
                    probeClass = probeMap.getOrDefault("class", "");
                    probeDiscriminator = probeMap.getOrDefault("discriminator_field", "");
                } else if ("classes".equals(key)) {
                    Map<String, Map<String, String>> obj = p.readObjectOfObjects();
                    for (Map.Entry<String, Map<String, String>> e : obj.entrySet()) {
                        Map<String, String> v = e.getValue();
                        classes.put(e.getKey(), new ClassDef(
                                v.getOrDefault("srg", ""),
                                v.get("obf")));
                    }
                } else if ("fields".equals(key)) {
                    Map<String, Map<String, String>> obj = p.readObjectOfObjects();
                    for (Map.Entry<String, Map<String, String>> e : obj.entrySet()) {
                        Map<String, String> v = e.getValue();
                        fields.put(e.getKey(), new FieldDef(
                                v.getOrDefault("owner", ""),
                                v.getOrDefault("srg", ""),
                                v.getOrDefault("type", "")));
                    }
                } else if ("methods".equals(key)) {
                    Map<String, Map<String, String>> obj = p.readObjectOfObjects();
                    for (Map.Entry<String, Map<String, String>> e : obj.entrySet()) {
                        Map<String, String> v = e.getValue();
                        methods.put(e.getKey(), new MethodDef(
                                v.getOrDefault("owner", ""),
                                v.getOrDefault("srg", ""),
                                v.getOrDefault("sig", "")));
                    }
                } else {
                    // 未知 object 段：跳过（读掉整个 object）
                    p.readFlatObject();
                }
            } else {
                // 字符串或数字值
                String value = p.readValue();

                switch (key) {
                    case "schema_version":
                        schemaVersion = Integer.parseInt(value.trim());
                        break;
                    case "mc_version":
                        mcVersion = value;
                        break;
                    case "namespace":
                        namespace = value;
                        break;
                    default:
                        // 忽略
                }
            }

            p.skipWs();
            if (p.peek() == ',') {
                p.next();
            }
        }

        if (schemaVersion != 2) {
            throw new IllegalStateException(
                    "不支持的 schema_version: " + schemaVersion + "（期望 2）");
        }
        if (mcVersion.isEmpty()) {
            throw new IllegalStateException("JSON 缺少 mc_version");
        }

        VersionConfig c = new VersionConfig(schemaVersion, mcVersion, namespace,
                probeClass, probeDiscriminator);
        c.classes.putAll(classes);
        c.fields.putAll(fields);
        c.methods.putAll(methods);
        return c;
    }

    private VersionConfig(int schemaVersion, String mcVersion, String namespace,
                          String probeClass, String probeDiscriminator) {
        this.schemaVersion = schemaVersion;
        this.mcVersion = mcVersion;
        this.namespace = namespace;
        this.probeClass = probeClass;
        this.probeDiscriminator = probeDiscriminator;
    }

    // ─── 简化 JSON Parser ───

    private static final class Parser {
        private final String s;
        private int pos;

        Parser(String s) {
            this.s = s;
            this.pos = 0;
        }

        char peek() {
            return pos < s.length() ? s.charAt(pos) : '\0';
        }

        // ★ 补上 next()
        char next() {
            return s.charAt(pos++);
        }

        void skipWs() {
            while (pos < s.length()) {
                char c = s.charAt(pos);
                if (c == ' ' || c == '\t' || c == '\n' || c == '\r') {
                    pos++;
                } else {
                    break;
                }
            }
        }

        void expect(char c) {
            skipWs();
            if (peek() != c) {
                throw new RuntimeException(
                        "JSON parse error at " + pos
                                + ": expected '" + c + "', got '" + peek() + "'");
            }
            pos++;
        }

        String readString() {
            skipWs();
            expect('"');
            StringBuilder sb = new StringBuilder();
            while (pos < s.length()) {
                char c = s.charAt(pos++);
                if (c == '\\') {
                    if (pos < s.length()) {
                        char esc = s.charAt(pos++);
                        switch (esc) {
                            case 'n': sb.append('\n'); break;
                            case 't': sb.append('\t'); break;
                            case 'r': sb.append('\r'); break;
                            case '"': sb.append('"'); break;
                            case '\\': sb.append('\\'); break;
                            case '/': sb.append('/'); break;
                            default: sb.append(esc); break;
                        }
                    }
                } else if (c == '"') {
                    return sb.toString();
                } else {
                    sb.append(c);
                }
            }
            throw new RuntimeException("JSON parse error: unterminated string at " + pos);
        }

        String readValue() {
            skipWs();
            if (peek() == '"') {
                return readString();
            }
            StringBuilder sb = new StringBuilder();
            while (pos < s.length()) {
                char c = s.charAt(pos);
                if (c == ',' || c == '}' || c == ']' || c == ' ' || c == '\n'
                        || c == '\r' || c == '\t') {
                    break;
                }
                sb.append(c);
                pos++;
            }
            return sb.toString();
        }

        Map<String, String> readFlatObject() {
            Map<String, String> result = new HashMap<>();
            expect('{');
            while (true) {
                skipWs();
                if (peek() == '}') {
                    next();
                    break;
                }
                String key = readString();
                expect(':');
                String value = readString();
                result.put(key, value);
                skipWs();
                if (peek() == ',') {
                    next();
                }
            }
            return result;
        }

        Map<String, Map<String, String>> readObjectOfObjects() {
            Map<String, Map<String, String>> result = new HashMap<>();
            expect('{');
            while (true) {
                skipWs();
                if (peek() == '}') {
                    next();
                    break;
                }
                String key = readString();
                expect(':');
                Map<String, String> inner = readFlatObject();
                result.put(key, inner);
                skipWs();
                if (peek() == ',') {
                    next();
                }
            }
            return result;
        }
    }

    // ─── 调试输出 ───

    @Override
    public String toString() {
        return "VersionConfig{mc=" + mcVersion
                + ", ns=" + namespace
                + ", classes=" + classes.size()
                + ", fields=" + fields.size()
                + ", methods=" + methods.size()
                + ", probe=" + probeClass + "/" + probeDiscriminator
                + "}";
    }
}