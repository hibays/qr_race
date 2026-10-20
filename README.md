# qr_race

多设备扫码接力系统：房主/副房主在真实房间点位扫一次码，房间内每台设备立刻拿到同一份内容，并用自己的屏幕重新亮出可被扫描的二维码。房间是固定场地地址，接力是一对多的码分发。

产品事实记录在 [PRODUCT.md](PRODUCT.md)；原始需求见 [PROMPT.md](PROMPT.md)。

## 现在有什么（M0–M30）

工作区、领域模型、扫码流水线与基准、鉴权与房间服务、HTTP/WS 传输、采集核心与桌面相机后端、参考客户端、Flutter 客户端（连接/大厅/房间/房主流程/扫码页/桌面与 Android 一键自部署）、原生扫描会话 FFI、SQLite 持久化、粗到精扫描流水线、Flutter Web 构建（wasm 扫码/QR 待 Emscripten 重做）、内置自签名 HTTPS、CLI 与 Flutter 的自签名证书信任（指纹固定）、Android App（内置 Rust 服务端、Camera2→Rust 扫码、权限/所有文件访问引导、二维码显示）、按前缀的自动发布与动态槽位、外部 App scheme 跳转、纯 Rust qrcode 生成二维码（2048² 无损 WebP，参数对齐 rcrm bridge）、程序化扰动纹理套件、WeChat Caffe 权重自行转 ONNX（SR 数值对齐，检测器 raw heads）、自实现微信 CNN 分层流水线（zxing-cpp 快路径 → CNN 384 → 全分辨率 crop → 小码 SR → 分块远码检测）已落地；Android 用 ORT；解码/WeChat crop 已统一为 zxing-cpp，Camera2 改为 YUV 彩色三平面；Android 彩色链路与 Apple 采集仍待真机重测。

```
crates/
├── qr_race_core/              领域：房间地址、角色权限、接力条目
├── qr_race_scan/              检测 →（按需）超分 → 解码；解码是 zxing-cpp，CNN 阶段是 trait
├── qr_race_qr/                qrcode crate 生成二维码：2048²、纠错 M、无损 WebP（q90/m4），对齐 rcrm
├── qr_race_capture/           采集核心：RGBA/BGRA→灰度、stride、30fps 定速与丢帧统计
├── qr_race_capture_desktop/   桌面相机后端（nokhwa：MF / V4L2 / AVFoundation）
├── qr_race_ffi/               原生扫描会话 C ABI：相机与扫描在 Rust 线程，Dart 只取命中与 JPEG 预览
├── qr_race_hub/               token 鉴权、房间/接力服务、房间事件
├── qr_race_store/             SQLite 持久化：房间、成员/角色、接力
├── qr_race_tls/               自签名证书生成/复用 + hyper-util TLS 服务循环
├── qr_race_api/               HTTP + WebSocket 传输：会话、房间 API、事件广播
├── qr_race_wechat/            自实现微信检测/超分流水线（Android ORT / 桌面 tract）
├── cvlite/                     vendored 上游图像缩放/颜色工具（WeChat pipeline 依赖）
├── qr_race_cli/               扫描客户端（bin: qr_scan）：图片或相机 → 登录 → 发布接力
├── qr_race_gui/               Flutter 客户端：连接、大厅、房间接力墙、扫码页（手机/桌面同套界面）
└── qr_race_bench/             基准 CLI：生成 fixture、跑耗时分布、输出 JSON
src/                           唯一的 server 二进制：serve / provision
DEPLOY.md                      部署与真机测试：一键自部署、USB/Wi-Fi、权限、排错
docs/API.md                    客户端契约（端点、状态码、事件、WS ticket）
docs/PERF.md                   实测与解读（含相机采集数据）
docs/ARCHITECTURE.md           客户端/相机/服务端/身份/动态码数据流与实测取舍
docs/VENDORED.md               cvlite/zxing-cpp 的 vendored 来源与构建状态
docs/PROMPT_CLOSURE.md        PROMPT.md 分层收尾清单、门禁与剩余缺口
docs/MODELS.md                 WeChat Caffe → ONNX：转换、验证与推理接入计划
docs/EXTENSIONS.md             扩展点说明（不含 Python/模组运行时）
docs/WASM.md                   wasm 扫码移除说明与 Emscripten 迁移缺口
docs/VERIFICATION.md           最终审计：门禁、功能证据与已知缺口
docs/ARCHITECTURE.md           客户端/相机/服务端/身份/动态码数据流与实测取舍
docs/MODELS.md                 WeChat Caffe → ONNX：转换、验证与推理接入计划
models/                        sr.onnx / detect.onnx（Apache-2.0，见 models/README.md）
tools/qr_bench.html            手工双码测试台（浏览器打开，用手机拍屏幕）
.refs/                         gitignored 的参考副本（WeChat 权重、wxscan）
```

## 鉴权与房间

- 用户由运营方 provision 出 bearer token；连接只校验 token，session 默认 12 小时。TOTP 已从产品流程移除。
- 连续 5 次错误 token 会把账户锁定 60 秒（返回 `429`），成功登录后计数清零。
- 房间状态机在 `qr_race_hub`：建暗房 → 点亮 → 加入 → 房主设副房主 → 房主或副房主发布接力；每次变更返回 `RoomEvent` 供传输层广播。
- 只有房主与副房主能发布接力；成员能看房间快照（谁在、什么身份、谁刚扫过），但发布会被拒绝。
- 暗房只有房主能进；成员必须等房间点亮。
- 传输层在 `qr_race_api`：REST 处理房间动作，WebSocket 每房间一条广播流；完整契约见 [docs/API.md](docs/API.md)。
- 浏览器无法给 WebSocket 加 Authorization 头，所以 WS 升级用 `POST /v1/ws-ticket` 换一次性 60 秒 ticket；原生端仍可用 bearer 头。
- relay 的 payload 以 base64 传输，事件带 relay 全文，客户端可直接重建二维码。
- 服务可直接以自签名 HTTPS 运行（`tls` 配置，启动打印证书指纹），也可由前置代理终止 TLS；明文 HTTP 仅建议用于可信内网。

## 采集

`qr_race_capture` 只做三件事，且都不依赖具体平台：

- `rgba_to_gray` / `bgra_to_gray`（含 stride 版本）把设备缓冲转成 `GrayFrame`；
- `Pacer` 按目标帧率决定收/丢（30fps → 33ms 间隔，整数除法宁慢勿超）；
- `CaptureSession` 把两者串起来，统计 `frames_in / frames_out / dropped`。

平台后端（capture-desktop / -android / -apple）只需实现 `CaptureSource::next_frame`，帧仍然不会进 Dart。

若扫码页预览显示"摄像头关闭"图标：到 Windows 设置 → 隐私和安全性 → 相机，允许桌面应用访问相机（本开发的会话里就遇到过一次，表现为全黑帧）。

## 扫码算法现状：分层，快路径 zxing-cpp + 按需微信 CNN

识别核心是分层的：

- **fast**：zxing-cpp 全帧（彩色 RGB，失败回退 luma）+ 经典 `PyramidDetector`；桌面合成套件 9–13ms，Android 彩色链路待真机重测；
- **CNN**：fast miss 后进入 WeChat 检测器（384×384），候选框回全分辨率 crop，再由 zxing-cpp 解码；小 crop 按需跑 SR；
- **远码**：每第 4 次 CNN 把画面切成 2×2 重叠块，各送 384 检测，保留小模块；
- **后端**：Android 用 ORT（`libonnxruntime.so`，真机 CNN 5.5–21.4ms）；桌面用 tract 兜底；
- **帧不进 Dart**：Camera2 Y 平面经 JNI 进 Rust，Dart 只轮询命中和统计。

实拍远码 `https://example.test/far-small-live` 在 1280×720 下命中，HUD 显示
桌面 zxing-cpp 套件 fast 约 10–13ms、CNN 待重测；普通清晰码由 fast path 处理，
不触发 CNN。实现位于 [crates/qr_race_wechat](crates/qr_race_wechat/src/lib.rs)，
vendored 来源见 [docs/VENDORED.md](docs/VENDORED.md)。

## 平台构建

| 目标 | 状态 | 命令 |
|---|---|---|
| Windows 桌面 | 可用（含原生相机 + FFI 扫码） | `flutter build windows --release` |
| Web | 构建可用；Rust wasm 扫码在 zxing-cpp 迁移后暂移除（需 Emscripten） | `flutter build web --release` |
| Android | **debug APK 可构建**；内置服务端（TLS/HTTP）、Camera2→Rust 扫码、自动发布、二维码显示、权限引导 | `pwsh -File scripts/android-deploy.ps1` |
| iOS / macOS | 未验证（需要 macOS 与 Xcode） | — |

Android 构建需要接受 SDK licenses（`flutter doctor --android-licenses`）。`scripts/android-deploy.ps1` 会先调用 `build-android-ffi.ps1` 交叉编译 arm64 `libqr_race_ffi.so`，再构建 APK；Gradle wrapper 若因 TLS 拦截报 PKIX，脚本会自动临时改用 `.refs/gradle-9.3.1-bin.zip`。Android App 可以在本机启动服务端（默认 18443）、用 Camera2 采集并由 Rust 直接扫码、自动发布；真机步骤见 [DEPLOY.md](DEPLOY.md)。

## 持久化

- `database` 配置指向 SQLite 文件；房间（成员、角色、加入密码哈希）与接力写穿到库，重启后原样恢复，接力 id 继续增长（不会重号）。
- 账号仍由运营方的 config 文件提供（token 不落库）；session 与 WS 订阅是进程内状态，重启后重新登录即可。
- 冒烟验证（真实进程）：建房 → 点亮 → 发布接力 → 停服 → 重启 → `GET /v1/room` 返回 `lit=true` 且接力字节逐字节一致。

## HTTPS（自签名）

配置里加 `tls` 即可直接跑 HTTPS，无需前置代理：

```json
"tls": { "cert": "cert.pem", "key": "key.pem", "hosts": ["localhost", "192.168.1.10"] }
```

- 两个文件都不存在时，首次启动生成自签名证书（SAN 取自 `hosts`，缺省 localhost/127.0.0.1/::1）并打印路径；之后复用同一份。
- 只缺一个文件会直接报错，不会静默替换——避免换掉客户端已信任的证书。
- 客户端需要信任 `cert.pem`：浏览器接受警告、`curl --cacert cert.pem`，或把证书装进设备根证书。
- 服务端启动会打印 `certificate SHA-256: <64 位 hex>`。CLI 用 `qr_scan --cacert cert.pem` 连接（本地调试也可 `--insecure`）；Flutter 连接页把该指纹填进"证书指纹（可选）"，原生端按指纹固定（`badCertificateCallback`），Web 端仍由浏览器接受一次证书。
- 冒烟验证：`curl -k` 与 `curl --cacert cert.pem` 均返回 200；Python 用该证书校验后 `POST /v1/auth` 成功拿到 session。
- 浏览器扫码（`getUserMedia`）要求安全上下文，用这套 HTTPS 或 localhost 即可。

## 命令

```powershell
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all -- --check

# 给用户签 token（把输出的 JSON 行贴进 config.json 的 accounts）
cargo run -q -p qr_race_server -- provision --user host-a

# 管理端账号：服务端治理权限（无模组上传）
cargo run -q -p qr_race_server -- provision --user admin --admin

# 起服务
cargo run -q -p qr_race_server -- serve --config config.json

# 扫描一张图并把结果发布成接力（Flutter 客户端要走的同一条路径）
cargo run -q -p qr_race_cli -- --server http://127.0.0.1:8080 `
  --token <token> --room 东京A区1B级103房 `
  --image .refs/demo.png --kind url

# 自签名 HTTPS：信任服务端证书（或 --insecure 仅限本地调试）
cargo run -q -p qr_race_cli -- --server https://127.0.0.1:8443 `
  --cacert cert.pem --token <token> --code 123456 --room 东京A区1B级103房 `
  --image .refs/demo.png --kind url

# 相机：列设备 / 只扫不发 / 指定 720p 后发布
cargo run -q -p qr_race_cli -- --list-cameras
cargo run -q -p qr_race_cli -- --camera 0 --frames 30 --probe
cargo run -q -p qr_race_cli -- --camera 0 --width 1280 --height 720 `
  --server http://127.0.0.1:8080 --token <token> --code 123456 `
  --room 东京A区1B级103房 --kind url

# Flutter 客户端（手机与桌面同一套界面）
cd crates\qr_race_gui
flutter pub get
flutter test
flutter run -d windows
# 无服务器预览界面
flutter run -d windows --dart-define=DEMO=true

# Web 构建（Rust wasm 扫码暂缺；需 Emscripten 重做）
flutter build web --release

# 原生扫描会话（cdylib，Dart FFI 用）
cargo build -p qr_race_ffi --release

# 把扫描器打进 Flutter 的 Windows 构建
cd crates\qr_race_gui
flutter build windows --release
pwsh scripts\copy_ffi.ps1 -Config Release

# 无界面验证 FFI（需要已构建的 DLL 与相机）
dart run tool/ffi_probe.dart

# 干净单码 fixture
cargo run --release -p qr_race_bench -- --generate .refs/demo.png

# 相机尺寸场景：1280×720，200px 的码
cargo run --release -p qr_race_bench -- --generate .refs/scene.png --scene `
  --width 1280 --height 720 --qr-size 200 --rotate 0

# 测量
cargo run --release -p qr_race_bench -- --input .refs/scene.png --iterations 200
cargo run --release -p qr_race_bench -- --input .refs/scene.png --iterations 200 --json
```

## 基准（实测）

Windows x64，`--release`，合成干净帧，未接入 CNN 检测器（耗时全部落在 zxing-cpp 自带的多码检测+解码）。

| fixture | 分辨率 | p50 | p95 | 由 p50 推算 |
|---|---|---|---|---|
| 干净单码 | 198×198 | 0.131 ms | 0.202 ms | — |
| 场景内 200px 码 | 1280×720 | 4.717 ms | 5.719 ms | ~212 fps |
| 同上，旋转 30° | 1280×720 | 4.738 ms | 5.416 ms | ~211 fps |
| 场景 + `--try-harder` | 1280×720 | 4.974 ms | 5.978 ms | ~201 fps |

这是解码路径的地板，不是 30fps 的最终证明：真实相机帧有噪声、运动模糊、光照变化与多码，CNN 检测与按需超分还要占用预算。相机权限开放后，真实纹理帧实测 1080p 全帧扫描 6.01ms、720p 2.89ms（无二维码入镜，采集约 24–28fps）；占位帧时期的数据与早先 debug 数据作废记录见 [docs/PERF.md](docs/PERF.md)。

粗到精流水线（同一 1280×720 场景）：整帧解码 4.63ms → 金字塔检测 2.33ms（解码降到 0.13ms）→ 金字塔 + 按需双线性超分 4.04ms（超分 1.32ms）。检测器与超分器都是 trait 实现，CNN 权重到位后可直接替换。

## 设计约束（摘自 PRODUCT.md）

- 单码检测+解码必须 < 33ms，取景 30fps+；超分只在检测框低于面积阈值时触发。
- 帧不进 Dart：采集、推理、解码都留在原生层。
- 解码用 zxing-cpp Rust binding（vendored 0.5.3，Apache-2.0，bundled C++）；二维码生成用纯 Rust `qrcode` crate；不再依赖 rxing/wxing。
- WeChat 开源的检测/超分权重是 Caffe 格式，Apache-2.0（含 zxing 归属），商用可用、需保留许可与归属；接 ONNX 后端时要处理转换或自带解析。

## 路线

- [x] M0 工作区 / 领域模型 / 扫码流水线 / 基准
- [x] M1 token 鉴权 / 房间与接力服务 / 事件
- [x] M2 HTTP/WS 传输：会话、房间 API、事件广播、管理端 provision
- [x] M3 WebSocket 真实 socket E2E（一次性 ticket）+ 采集核心
- [x] M4 参考客户端 `qr_scan`：扫码 → 发布 → 成员收到，端到端测试
- [x] M5 桌面采集后端（nokhwa）+ 实机 720p 扫描路径
- [x] M6 Flutter 客户端骨架：连接 / 大厅 / 房间接力墙 + 成员条，手机与桌面双布局
- [x] M7 Flutter 房主流程：创建并点亮房间、暗房提示、副房主升降级
- [x] M8 原生扫描会话 FFI（cdylib）：start / stop / state / preview，帧不进 Dart
- [x] M9 Flutter 扫码页：FFI 预览 + 命中列表 + 一键发布接力
- [x] M10 SQLite 持久化：房间/成员/角色/接力跨重启恢复
- [x] M11 粗到精扫描：金字塔检测 → 裁剪解码（并行）+ 按需双线性超分
- [x] M12 Flutter Web 构建通过（wasm dry-run）+ API CORS；扫码页在 Web 上明确提示未接入
- [x] M13 wasm 扫描内核编译通过 + C ABI（帧内存由 JS 写入，JSON 命中读出）
- [x] M14 wasm 扫码在 Node 验证通过（1280×720 约 18–22ms）+ Flutter Web 相机 → wasm 扫码接入
- [x] M15 内置自签名 HTTPS：首次生成/复用证书，HTTP 仍可用
- [x] M16 浏览器相机 → wasm 扫码端到端（Edge headless 假摄像头，扫描 ~1.8ms）
- [x] M17 扩展点收敛：核心 trait/事件/协议可替换；不实现 Python 模组运行时
- [x] M18 自签名 HTTPS 客户端信任：CLI `--cacert/--insecure`，Flutter 证书指纹固定
- [x] M19 房间 UI：成员、角色、接力与邀请流程
- [x] M20 管理员只保留服务端治理权限，不承担模组上传
- [x] M21 登录防爆破：连续 5 次失败锁定账户 60 秒（429），成功即清零
- [x] M22 Flutter Android debug APK 构建通过（原生采集/Rust 扫描库尚未打包）
- [x] M23 真实相机帧验证：1080p 6.01ms / 720p 2.89ms，采集约 24–28fps（无码入镜）
- [x] M24 二维码生成改用纯 Rust `qrcode`：2048² 无损 WebP（q90/m4），FFI 两路，移除 qr_flutter
- [x] M25 程序化扰动纹理套件：10 个场景（噪声/模糊/低对比/梯度/旋转/透视/小码/多码/相机混合），bench --suite 与测试共用
- [x] M26 Caffe→ONNX：自行转换 SR/检测器；SR 与独立 NumPy 前向最大误差 5.5e-06，检测器 prior decode/NMS 已在 qr_race_wechat 实现
- [x] M27 桌面客户端一键自部署：自动 provision 服务器身份、写 TLS/SQLite 配置、启动服务器、展示 URL/指纹/token，并可直接用主机账号连接
- [x] M28 Android 真机：arm64 Rust 库打包（原生二维码显示）、相机权限弹窗 + 拒绝后“去设置”引导、adb reverse 下 TLS 指纹登录成功
- [x] M29 Android 内置服务端：`qr_race_embedded` 编译进 `libqr_race_ffi.so`，UI 可选自签名/已有证书/HTTP，默认 18443；真机 TLS 指纹登录成功
- [x] M30 Android Camera2 → JNI → Rust 扫描：720×1280 预览、约 6ms/帧，实拍二维码命中；自动发布、前缀动态槽位、外部 App scheme 跳转、连接链接预填
- [x] M31 每房间动态成员：房主专用 `/v1/room/guest` 生成临时身份，邀请链接直接进房；服务端不再要求预置“成员账号”
- [x] M32 桌面自部署进 bridge：Rust 通过 Windows Job Object 持有 server 子进程，关闭 GUI 自动清理；配置/SQLite 复用；桌面 CJK 字体回退对齐 rcrm；TOTP 2FA 从产品与初版需求移除
- [x] M33 自实现 WeChat 分层：ORT CNN 384 → 全分辨率 crop → 小码 SR → 2×2 分块远码检测；vendored cvlite；真机旧 rxing 数据 fast p50 6.8ms、CNN p50 5.5ms、远码分块 p50 21.4ms（zxing-cpp/彩色相机需重测）
- [ ] Apple 采集（AVFoundation）
- [ ] Web/wasm 实时采集扫码（已有内核/探针，浏览器矩阵未全验）
- [ ] 真实相机 p95/丢帧 trace 与远码检测率 sweep

















