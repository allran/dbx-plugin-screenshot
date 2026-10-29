# 本地调试

```bash
nvm use 22
npm install
dbx-plugin dev --path . --port 5190
```

截屏权限与真实显示器只能在 **DBX 桌面宿主**验证；dev host 不模拟屏幕录制授权。

## 本地打包（当前平台）

```bash
cd /Users/alan/VsCodeProjects/alan/dbx-plugin-screenshot
nvm use 22
npm run build
dbx-plugin package .
```

产物在 `dist/`，例如 `io.dbx.screenshot-0.1.0-darwin-arm64.dbxp`。

## 装进 DBX

1. 插件中心 → 设置 → 第三方与开发者选项 → 允许安装未签名开发包
2. 安装 `dist/` 里的 `.dbxp`
3. macOS：系统设置 → 隐私与安全性 → **屏幕录制** → 勾选 DBX → 重启 DBX
4. 打开「截图」工作台，点「截取」

## RPC

| 方法 | 作用 |
| --- | --- |
| `screenshot/permission` | 查询屏幕录制权限状态（只读，不弹系统框） |
| `screenshot/permission/request` | `{ confirmed: true }` 才触发系统授权（须先插件内确认） |
| `screenshot/permission/open-settings` | `{ confirmed: true }` 才打开系统设置（须先插件内确认） |
| `screenshot/displays` | 列出显示器 |
| `screenshot/windows` | 列出可截取窗口（过滤控制中心等系统 UI，不含预览） |
| `screenshot/window/preview` | `{ id }` 异步生成单个窗口缩略图 |
| `screenshot/capture` | `{ mode: "display"\|"window"\|"region", displayIndex?, windowId?, delaySecs? }` |
| `screenshot/list` | 历史列表 |
| `screenshot/copy` | 复制到系统剪贴板 |
| `screenshot/media` | lightbox data URL |
| `screenshot/reveal` | 在文件夹中显示 |
| `screenshot/favorite` / `delete` / `clear` | 收藏 / 删除 / 清空 |

区域截图（macOS）走系统 `screencapture -i` 框选；Esc 取消。
