# Versioning and Compatibility

## 生命周期

`v1alpha1 -> v1beta1 -> v1`。Alpha 允许 breaking change，但每次必须更新 changelog、Schema、fixture 和 migration note。Beta 仅在有重大证据时 breaking。Stable 使用兼容演进或新版本并存。

## 两个版本轴

Package/app version 与 protocol `apiVersion` 独立。Desktop 版本不能作为 capability availability 代理。

## Deprecation

- 发布 deprecation notice 与替代 coordinate。
- 至少保留两个已声明兼容周期，除非存在安全紧急情况。
- Adapter 同时支持新旧版本时，优先协商最高共同版本。
- 删除前更新 compatibility matrix、migration 和 tracking interface。

## 私有命名空间

M0 采用 `*.dsh-desktop.local/v1alpha1`，不得宣称为 dsh-std 正式协议。未来标准坐标通过 adapter 映射。
