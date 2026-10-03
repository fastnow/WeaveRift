# versions/*.json — 版本配置

## 命名规则

`<mc_version>.json` — 一个版本一个文件，不区分 loader。

**loader 决定的是解析策略，不是数据本身**：
- vanilla：运行时用 `obf` 名，需 SRG 文件反查
- forge：运行时用 `srg` 名，直接用
- 26.1+：运行时用 `mojmap` 名，直接用

**Fabric 从 1.14 起，1.12.2 只有 vanilla / forge。**

## Schema

```json
{
  "schema_version": 2,
  "mc_version": "1.12.2",
  "namespace": "srg",

  "classes": {
    "<逻辑名>": {
      "srg": "全限定类名",
      "obf": "混淆名"
    }
  },

  "fields": {
    "<Class>.<srg>": {
      "owner": "逻辑类名",
      "srg": "field_xxx",
      "type": "逻辑类型或基本类型"
    }
  },

  "methods": {
    "<Class>.<srg>": {
      "owner": "逻辑类名",
      "srg": "func_xxx",
      "sig": "SRG 形式签名"
    }
  },

  "probe": {
    "class": "Minecraft",
    "discriminator_field": "Minecraft.field_71439_g"
  }
}