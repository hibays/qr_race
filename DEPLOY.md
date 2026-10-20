# 部署与真机测试（Ubuntu + Windows + Android）

这份文档从一台 Ubuntu 中心服务器或一台 Windows 电脑 + 一台安卓手机开始。先选一条路：

| 我想做什么 | 用哪个 | 需要服务器吗 |
|---|---|---|
| 多设备共用中心服务器 | Ubuntu + `deploy-server-linux.sh` | 需要（自签名 HTTPS） |
| 手机自己开服并扫码 | Android App →「自部署服务器」 | 不需要电脑 |
| 手机连电脑房间 | 桌面版「自部署服务器」+ 手机 App | 需要（电脑上自动启动） |
| 只验证扫码/二维码 | CLI `qr_scan` / `qr_bench` | 不需要服务器 |
| 完全离线看 UI | 桌面版 `DEMO=true` | 不需要 |

> Android 现在可以：在 App 内启动编译进 `libqr_race_ffi.so` 的服务端（TLS 自签名/已有证书/HTTP 可选）、用 Camera2 采集并由 Rust 直接扫码、识别后自动发布、按前缀动态更新槽位、点击结果跳转微信/企业微信/浏览器。相机权限运行时申请；读取任意目录的证书需要“所有文件访问权限”。
>
> **扫码内核是分层算法**：zxing-cpp 彩色快路径；miss 时进入微信 CNN 检测器 + 全分辨率 crop + 小码 SR，再由 zxing-cpp 解码。768 检测只按调度触发，避免拖垮 30fps 快路径。详见 [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) 与 [docs/VENDORED.md](docs/VENDORED.md)。

## 0. Ubuntu 中心服务器（推荐）

```bash
sudo apt install -y build-essential pkg-config libssl-dev python3
cargo build --release -p qr_race_server

# 安装为 systemd 服务；数据在 /var/lib/qr_race，--host 写进证书 SAN
sudo PUBLIC_HOST=race.example.lan \
  ./scripts/deploy-server-linux.sh --port 8443 --host race.example.lan
```

脚本生成 host token（已存在则复用）和自签名 `cert.pem/key.pem`，启动 systemd 服务，打印：

```
server URL:   https://<host>:8443
host token:   ...
fingerprint:  <64 hex>
```

指纹也可单独读取：

```bash
qr_race_server fingerprint --cert /var/lib/qr_race/cert.pem
```

客户端填 HTTPS URL + 指纹 + token。自签名链接若没有 64 位 hex 指纹，构造时直接拒绝，避免生成必然握手失败的链接。无 systemd 时用 `--foreground`；防火墙放行 8443/TCP，`config.json`/`key.pem` 按密码文件保护。

## 1. 最快路径：桌面版一键自部署

1. 构建服务端和桌面客户端（在仓库根目录）：

   ```powershell
   cargo build --release -p qr_race_server
   cd crates\qr_race_gui
   flutter run -d windows
   ```

   自部署按钮会按以下顺序找 `qr_race_server.exe`：环境变量 `QR_RACE_SERVER` → 桌面程序同目录 → 项目 `target\release` / `target\debug`。

2. 在桌面 App 的连接页点 **“自部署服务器（本机）”**，端口保持 `8443`，点 **“启动本机服务器”**。

   它会自动完成：生成服务器身份 → 写 TLS 配置 → 启动服务器 → 显示 URL、证书指纹、账号 token。

3. 点 **“用此账号连接”**。进入大厅后创建/进入房间；房间顶部扫码按钮用于桌面扫码发布。成员不预置账号：进房间后点 **“邀请成员”**，服务端会为该房间动态生成一个临时成员身份并给出邀请链接。

   ![Android 已登录大厅](assets/android-lobby.png)

   ![Android 房间里的二维码](assets/android-room.png)

4. 手机加入时，让对方打开房主复制的 **邀请链接**：链接会填好服务器、指纹、token，自动登录并直接进入该房间。

   - 手机需要先安装 APK：见第 2 节。
   - 邀请身份只属于这个房间；房主可以随时再生成新的邀请。
   - 邀请链接含 token，按密码对待。

5. 服务端日志会打印监听地址和证书指纹；自部署页面也会显示。桌面自部署由 Rust bridge 在 Windows Job Object 中持有子进程：**关闭 GUI 会自动杀掉服务端并释放端口**，不需要手动清理。配置、账号和 SQLite 库保存在 `%APPDATA%\qr_race\selfhost\desktop`，下次启动复用。

## 2. 安卓本机自启动（无需电脑）

1. 安装 APK：

   ```powershell
   pwsh -File scripts\android-deploy.ps1
   ```

   脚本会先编译 arm64 Rust 库、再构建/安装 APK（`-SkipRust` 可跳过前者）。

2. 打开 App → 连接页 → **“自部署服务器（本机）”**。

   - 端口默认 **18443**（8443 在部分手机上已被系统/VPN 占用，可直接改）。
   - 连接方式三选一：**自签名**（默认，自动生成 cert.pem/key.pem）、**已有证书**（选 cert/key 文件）、**仅 HTTP**（只建议本机调试）。
   - 需要读任意目录的证书时，点“去申请”打开系统“所有文件访问权限”；文件选择器本身会把选中的文件复制到应用缓存。
   - 点 **“启动本机服务器”**。页面会显示 `https://127.0.0.1:18443`、证书指纹、服务器账号 token。

3. 点 **“用此账号连接”**。服务器在 App 进程内运行，只监听 127.0.0.1；退出 App 后进程结束，房间/接力是内存态（重启清空）。**单机自启动不适合多设备邀请**（邀请链接是 127.0.0.1）；多设备请用桌面/独立服务器。

   ![Android 内置 TLS 服务端](assets/android-selfhost.png)

4. 第二台手机：房主进房间后点 **“邀请成员”**，复制生成的邀请链接发过去；对方打开后自动登录并进入该房间（成员身份只在这个房间有效）。

   ![连接链接预填（截图为示例数据：example.test 不是可用服务器，请用自部署页生成的链接）](assets/android-deeplink.png)

## 3. 安卓真机：USB（推荐，免防火墙）

前提：手机已开启开发者选项和 **USB 调试**，数据线连接后在手机上点“允许 USB 调试”。

```powershell
# 先确认设备已授权
D:\Dev\Android\Sdk\platform-tools\adb.exe devices
```

应看到类似：

```
3B163Q00LSK00000        device product:PLF110 model:PLF110 device:OP60F9L1
```

然后：

```powershell
# 端口要和自部署页一致（默认 8443）
pwsh -File scripts\android-deploy.ps1 -Port 8443
```

脚本会做四件事：`adb reverse tcp:8443 tcp:8443`、构建 debug APK、安装、启动 App。`adb reverse` 让手机把 `127.0.0.1:8443` 转发到电脑，因此**不需要同一 Wi-Fi、不需要改防火墙**。

手机 App 连接页填：

- 服务器地址：`https://127.0.0.1:8443`
- 证书指纹：自部署页面显示的那串 64 位 hex
- 访问 token：服务器账号 token（房主自己）；成员用房间邀请链接
- 手机端只需要服务器 URL、证书指纹和 token。

如果 `adb devices` 是空的：
1. 手机：设置 → 关于手机 → 连续点“版本号”7 次；返回设置 → 系统 → 开发者选项 → 打开 **USB 调试**。
2. 插线后下拉通知栏，USB 用途选 **文件传输/MTP**（不要“仅充电”）。
3. 手机上弹出“允许 USB 调试吗？”→ 允许（建议勾选始终允许）。
4. 电脑上重试：

   ```powershell
   & "D:\Dev\Android\Sdk\platform-tools\adb.exe" kill-server
   & "D:\Dev\Android\Sdk\platform-tools\adb.exe" start-server
   & "D:\Dev\Android\Sdk\platform-tools\adb.exe" devices
   ```

5. 仍为空：换一根支持数据传输的线/换 USB 口；Windows 设备管理器里应出现 Android ADB Interface。无线调试也可在开发者选项里配对后用 `adb connect <手机IP>:<端口>`，但 USB 最省事。

## 4. 安卓真机：同一 Wi-Fi

1. 自部署页或服务端日志里的 URL 若显示 `https://192.168.x.x:8443`，手机和电脑连同一个 Wi-Fi。
2. 电脑防火墙放行端口（管理员 PowerShell）：

   ```powershell
   New-NetFirewallRule -DisplayName "qr_race 8443" -Direction Inbound `
     -Protocol TCP -LocalPort 8443 -Action Allow
   ```

3. 手机连接页填该 URL + 指纹 + 账号。
4. 连不上先检查：手机和电脑是否同一网段、路由器是否开了 AP 隔离、Windows 网络是否被识别为“公用”（可临时改为专用）。

如果自部署页显示的是 `127.0.0.1`，说明没找到合适的局域网 IPv4；直接走第 2 节 USB 方式。

## 5. 没有服务器/不想部署时怎么测试

- **桌面 UI 演示**（不连服务器）：

  ```powershell
  cd crates\qr_race_gui
  flutter run -d windows --dart-define=DEMO=true
  ```

  直接进入示例房间，能看到二维码渲染、成员卡片等；扫码按钮会打开扫码页。

- **只测扫码引擎**（不需要服务器）：

  ```powershell
  cargo run --release -p qr_race_cli -- --image .refs\some-qr.png
  cargo run --release -p qr_race_cli -- --camera --probe
  ```

- **只测扰动纹理和性能**：

  ```powershell
  cargo run --release -p qr_race_bench -- --suite --detector pyramid
  ```

- **跑测试**：

  ```powershell
  cargo test --workspace
  cd crates\qr_race_gui
  flutter test
  ```

“离线扫码自测”页面已删除；不需要服务器时用上面的 CLI 或 DEMO 模式即可。

## 6. 手动部署（不用桌面 App）

适合把服务端跑在另一台机器/无 GUI 的 Windows 上。

```powershell
cargo build --release -p qr_race_server
pwsh -File scripts\deploy-server.ps1 -Port 8443
```

首次运行会：生成 `config.json`（含一个服务器账号、TLS、SQLite 路径）、生成证书、启动服务器。控制台会打印：

- `https://<局域网IP>:8443`
- 服务器账号 token
- 证书指纹（服务端启动后打印）

把 token/指纹发给手机端即可。`config.json` 等同密码文件，不要提交到仓库或发给无关的人。

也可以完全手动：

```powershell
# 1) 生成账号（各打印一行 JSON）
.\target\release\qr_race_server.exe provision --user host
.\target\release\qr_race_server.exe provision --user member

# 2) 写 config.json，例如：
# {
#   "listen": "0.0.0.0:8443",
#   "database": "data/qr_race.db",
#   "tls": { "cert": "data/cert.pem", "key": "data/key.pem",
#            "hosts": ["localhost", "127.0.0.1", "192.168.1.20"] },
#   "accounts": [ { "user_id": "host", "token": "...", "admin": false } ]
# }

# 3) 启动
.\target\release\qr_race_server.exe serve --config config.json
```

`hosts` 必须包含手机实际使用的地址（IP 或域名），否则 TLS 主机名校验会失败。

## 7. 自动发布、动态码与外部 App 跳转

- 扫码识别后**自动发布**，不需要点发布；同一前缀的新内容会在房间里替换旧内容，不同前缀同时保留。
  - 前缀写法：`A:https://...`、`cam1|payload`；URL 本身（`https://...`）不会被误判为前缀。
  - 房间按前缀去重，只显示每个前缀最新的一条；同时扫多个不同前缀的码会同时出现。

    ![Android 自动扫码](assets/android-scan.png)
- 每条识别结果/房间卡片右侧有 **打开** 图标：
  - http(s) 走 Android 系统选择器，已安装的微信/企业微信/浏览器都可选；
  - `weixin://`、`wxwork://` 等自定义 scheme 直接拉起对应 App；
  - 没有 App 能处理时自动复制链接，避免死路。
- 网络方案：
  - 手机自启动 → 只用 loopback，不需要局域网/防火墙；
  - 电脑自启 → USB `adb reverse`（推荐）或同 Wi-Fi + 防火墙放行；
  - 外部 App 跳转需要目标 App 和目标链接本身可访问的网络。

## 8. 常见问题

| 现象 | 原因/处理 |
|---|---|
| 手机连接页提示证书/握手失败 | 指纹填错，或 URL 的 IP 不在证书 `hosts` 里。重新复制自部署页的指纹；USB 方式用 `127.0.0.1`。 |
| 连接返回 401 | token 错或已失效；重新复制 token。 |
| 连续 5 次 401 后返回 429 | 服务端锁定 60 秒，等一分钟再试。 |
| 手机打不开 `https://192.168...` | 防火墙未放行、不同 Wi-Fi、AP 隔离，或服务端监听在 127.0.0.1。 |
| `adb devices` 空 | 见第 3 节：USB 调试、MTP 模式、RSA 允许、重启 adb。 |
| 自启动报 `Address already in use` | 8443 在部分手机被占用；把端口改成 18443（默认）或任意 1024–65535。 |
| 扫码页黑屏/一帧一闪 | 确认相机权限已允许；预览会保留上一帧（不再闪黑）。仍异常时重启 App 并检查其他相机应用是否占用。 |
| 扫码页一直“正在申请相机权限…” | 系统权限弹窗被遮挡，回到桌面再切回 App；或在设置里检查权限。 |
| 拒绝相机权限后没有弹窗 | 页面会给出“重新申请/打开系统设置”，到设置 → 应用 → qr_race → 权限 手动打开。 |
| Android 房间二维码是空白/图标 | APK 必须包含 arm64 的 `libqr_race_ffi.so`；用 `scripts\android-deploy.ps1` 重新构建安装，不要装旧 APK。 |
| Gradle 构建报 PKIX / 证书错误 | 本仓库的脚本会自动临时使用 `.refs\gradle-9.3.1-bin.zip`；手动构建时也可临时把 `android/gradle/wrapper/gradle-wrapper.properties` 的 `distributionUrl` 指向该本地 zip，构建后还原。 |

## 9. 安全提醒

- 自签名 HTTPS 只适合内网/自用；对外部署请换正式证书，不要把 8443 直接暴露到公网。
- 连接链接里含 token，等同登录凭据；不要公开转发。
- “所有文件访问权限”只用于读取你手选的证书；文件选择器会复制到应用私有缓存。
- `config.json`、`cert.pem`、`key.pem`、`qr_race.db` 都含敏感信息，不要提交。
- 管理员账号可以上传模组，只发给可信的人。
- 当前 App 允许明文 HTTP 仅用于本地调试；正式使用请走 HTTPS。


