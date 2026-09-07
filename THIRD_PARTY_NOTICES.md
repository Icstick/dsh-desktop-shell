# Third-Party Notices

DSH Desktop Shell 分为两类授权：**应用代码**（Apache-2.0）与**第三方视觉素材**
（CC BY-NC-SA 4.0，独立授权、不覆盖代码）。

## 第三方视觉素材（CC BY-NC-SA 4.0）

以下素材**不属于 Apache-2.0**，按 CC BY-NC-SA 4.0（署名-非商业性使用-相同方式共享）随包分发。
非商业免费分发允许；商业用途或修改分发请先取得原作者授权。

目录：`apps/desktop/features/shell-ui/src/assets/`（详细出处见该目录 `ATTRIBUTION.md`）

| 素材 | 用途 | 作者/来源 | 状态 |
|---|---|---|---|
| deepseek-water | 欢迎页壁纸 | ZipZipPipe（上善无形），bilibili opus 1238515173432492049 | 原样分发 |
| deepseek-study | 用量页横幅 | 基于 ZipZipPipe 角色设定（bilibili opus 1238267551429951496）AI 衍生 | 演绎（同 CC BY-NC-SA 发布） |
| deepseek-workshop / explorer / correspondence | 设置/浏览器/通知页横幅 | 同上 AI 衍生 | 演绎（同 CC BY-NC-SA 发布） |
| harness-observatory | 运行时页横幅 | 基于 bilibili opus 1236362986801594377 参考 AI 衍生 | 演绎（同 CC BY-NC-SA 发布） |

许可链接：https://creativecommons.org/licenses/by-nc-sa/4.0/

### 隔离说明

- 素材以独立 `assets/` 目录存放并单独声明授权；**不传染**应用代码的 Apache-2.0。
- ShareAlike 仅约束素材本身的演绎（Adapted Material），不约束调用它们的 TypeScript/Rust 代码。
- 若项目未来转向商业用途，可整体移除/替换 `assets/` 素材而不影响代码授权链。
- 发行物中素材以 libwebp 压缩版本打包；原始 PNG 保留在仓库（不被安装包引用）。
