package com.fastnow.weaverift;

import java.util.Map;
import java.util.regex.Matcher;
import java.util.regex.Pattern;

/**
 * 名空间解析器：把 SRG 名转成运行时实际名字。
 *
 * <p>三种名空间的转换规则：
 * <ul>
 *   <li><b>SRG</b>（Forge）：类名、字段名、方法名、签名原样使用</li>
 *   <li><b>OBF</b>（原版）：类名查 CLASS_MAP，字段名查 FIELD_MAP，方法名/签名查 METHOD_MAP</li>
 *   <li><b>MOJMAP</b>（1.20.1+）：暂不实现，抛异常</li>
 * </ul>
 *
 * <p>依赖 {@link WeaveRiftAgent} 里已加载的 SRG 表（同包访问）。
 */
public final class NamespaceResolver {

    private final VersionProbe.Namespace namespace;
    private final Map<String, String> classMap;
    private final Map<String, String> fieldMap;
    private final Map<String, WeaveRiftAgent.MethodMapping> methodMap;

    public NamespaceResolver(VersionProbe.Namespace ns,
                             Map<String, String> classMap,
                             Map<String, String> fieldMap,
                             Map<String, WeaveRiftAgent.MethodMapping> methodMap) {
        this.namespace = ns;
        this.classMap = classMap;
        this.fieldMap = fieldMap;
        this.methodMap = methodMap;
    }

    public VersionProbe.Namespace namespace() {
        return namespace;
    }

    // ─────────────────── 类名 ───────────────────

    /**
     * SRG 全限定名 → 运行时类名（用于 Class.forName）。
     *
     * @param srgFullName 比如 "net/minecraft/client/Minecraft"（斜杠分隔）
     * @return SRG 下原样返回；OBF 下返回 "bib"（点分隔）
     */
    public String mapClass(String srgFullName) {
        switch (namespace) {
            case SRG:
                return srgFullName.replace('/', '.');

            case OBF: {
                String obf = classMap.get(srgFullName);
                if (obf == null) {
                    // 没查到，可能是未混淆的内部类（比如 net/minecraft/util/...）
                    return srgFullName.replace('/', '.');
                }
                return obf.replace('/', '.');
            }

            case MOJMAP:
                throw new UnsupportedOperationException(
                        "MOJMAP 名空间未实现（1.20.1 卡点在 core profile 渲染，不在此处）");

            default:
                return srgFullName.replace('/', '.');
        }
    }

    // ─────────────────── 字段名 ───────────────────

    /**
     * SRG owner 全名 + SRG 字段名 → 运行时字段名。
     *
     * @param srgOwnerFull 比如 "net/minecraft/client/Minecraft"
     * @param srgFieldName 比如 "field_71439_g"
     * @return SRG 下原样返回；OBF 下返回 "h"
     */
    public String mapField(String srgOwnerFull, String srgFieldName) {
        switch (namespace) {
            case SRG:
                return srgFieldName;

            case OBF: {
                String srgPath = srgOwnerFull + "/" + srgFieldName;
                String obfPath = fieldMap.get(srgPath);
                if (obfPath == null) {
                    // 没查到，可能是未混淆字段
                    return srgFieldName;
                }
                return obfPath.substring(obfPath.lastIndexOf('/') + 1);
            }

            case MOJMAP:
                throw new UnsupportedOperationException("MOJMAP 名空间未实现");

            default:
                return srgFieldName;
        }
    }

    // ─────────────────── 方法名 + 签名 ───────────────────

    /**
     * SRG owner 全名 + SRG 方法名 + SRG 签名 → 运行时方法名 + 签名。
     *
     * @param srgOwnerFull 比如 "net/minecraft/client/Minecraft"
     * @param srgMethodName 比如 "func_71410_x"
     * @param srgSig 比如 "()Lnet/minecraft/client/Minecraft;"（SRG 形式）
     * @return SRG 下原样返回；OBF 下返回 (obfName, obfSig)
     */
    public WeaveRiftAgent.MethodMapping mapMethod(String srgOwnerFull,
                                                   String srgMethodName,
                                                   String srgSig) {
        switch (namespace) {
            case SRG:
                return new WeaveRiftAgent.MethodMapping(srgMethodName, srgSig, srgSig);

            case OBF: {
                String srgPath = srgOwnerFull + "/" + srgMethodName;
                WeaveRiftAgent.MethodMapping mm = methodMap.get(srgPath);
                if (mm != null) {
                    return mm;   // 已经是 (obfName, obfSig, srgSig) 三元组
                }
                // 没查到，可能是未混淆方法（比如 getVersion）
                // 签名里的类名需要重映射
                String obfSig = remapSig(srgSig);
                return new WeaveRiftAgent.MethodMapping(srgMethodName, obfSig, srgSig);
            }

            case MOJMAP:
                throw new UnsupportedOperationException("MOJMAP 名空间未实现");

            default:
                return new WeaveRiftAgent.MethodMapping(srgMethodName, srgSig, srgSig);
        }
    }

    // ─────────────────── 签名重映射 ───────────────────

    /**
     * 把 SRG 签名里的类名替换成运行时类名。
     *
     * <p>比如 {@code "()Lnet/minecraft/client/Minecraft;"}
     * → OBF 下 {@code "()Lbib;"}
     *
     * <p>基本类型（{@code ()F}）不受影响。
     */
    public String remapSig(String srgSig) {
        if (srgSig == null || srgSig.isEmpty()) {
            return srgSig;
        }
        if (namespace == VersionProbe.Namespace.SRG) {
            return srgSig;
        }
        if (namespace == VersionProbe.Namespace.MOJMAP) {
            throw new UnsupportedOperationException("MOJMAP 名空间未实现");
        }

        // 匹配 L<类名>; 形式（类名里可以有 / 和 $）
        StringBuilder out = new StringBuilder();
        Matcher m = Pattern.compile("L([^;]+);").matcher(srgSig);
        int last = 0;
        while (m.find()) {
            out.append(srgSig, last, m.start());
            String srgClass = m.group(1);
            String mapped = mapClass(srgClass);
            out.append('L').append(mapped).append(';');
            last = m.end();
        }
        out.append(srgSig, last, srgSig.length());
        return out.toString();
    }
}