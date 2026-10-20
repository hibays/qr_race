# Product

<!-- impeccable:product-schema 1 -->

## Platform

adaptive

一个 Flutter 产品覆盖 Android（arm64-v8a、armeabi-v7a、x86_64）、iOS、macOS、Linux、Windows 与 Web（wasm）。手机与桌面是同等重要的两套布局基准；记录为 adaptive 是因为同一产品要在手持与桌面两种形态上同时成立，不代表 Material/Cupertino 双皮肤已被采纳。

## Stack

Rust workspace（edition 2024）。`crates/` 下按职责拆 crate，最外层 `src/` 是唯一的 server 二进制。GUI 用 Flutter 实现多平台界面，wasm 是一等目标。扫码后端按 target 用 Cargo features 或分 crate 提供"同功能、不同实现"，而不是把平台分支塞进运行时。

## Users

- **房主（host）**：现场社群活动的组织者，或运营方指派的骨干。手持设备在真实房间点位扫码、发布接力，指定副房主。
- **副房主（co-host）**：房主授权、可扫码发布接力的成员。其"已扫码"的用户特征被特化，其他人点其头像即可接力。
- **普通成员（member）**：凭 token 加入逻辑房间的受邀用户。通常不在房主所在的物理房间，通过接力拿到内容并在自己设备上亮码，完成后可退出。
- **管理员（admin）**：运营 server、发放 server URL 与 auth token 的一方。admin 仅保留为服务端治理权限，不再承担模组上传。

产品按 production-ready 的受管服务设计，同时服务两类场合：现场社群活动（展会、漫展、社团、场馆），以及运营方发放账号体系的持续使用。

## Product Purpose

在活动现场，大多数人并不站在那枚码前面。这个产品让房主（或副房主）在源点扫一次，房间内每台设备立刻拿到同一份内容，并用自己的屏幕重新亮出一枚可被扫描的二维码。它把"一次扫描"变成"全房间可用的接力件"，而不是一条聊天记录。

成功的样子：取景持续 30fps 以上，一帧内多码同扫，解码不阻塞取景；房主扫完的一个节拍内，房间内所有设备都出现接力内容，现场任何人都不必走到源点。

## Positioning

- **自实现的 WeChat 式扫码后端**：检测 →（按需）超分 → 解码，整条链路留在原生层，帧不进 Dart；多码并行解码，超分只在检测框过小时触发。微信 CNN 仅保留自实现检测/超分/分层；解码统一用 zxing-cpp，TFLite 后端不采用。
- **房间是真实场地的固定地址**：两京（东京、西京）互为平级，各京下辖区、区下分级、级下分房，同级实体之间平级；例如"东京A区1B级103房"。每房有固定识别码，用于构造有效鉴权路径。房间是固定盘点资源：可以随机新建，也可以点亮既有的未激活房间；激活房间在大厅显示。
- **接力是一对多的码分发**：扫码、端侧解码、压缩上传、服务端广播、成员设备重建二维码与内容，一条链把码送到不在现场的人手上。

## Operating Context

- 用户从运营方获得 server URL 与 auth token，在客户端输入连接；server 端只校验 token，鉴权成功后才可用。部署两者都要：可自签名 HTTPS，也可纯 HTTP；客户端只认一个 URL。
- 房主建立房间或点亮未激活房间；可设加入密码并随时撤销，可指定多个副房主。
- 扫码只发生在房主/副房主这一端（单端扫码）：端侧解码后压缩上传到 server，server 广播到房间内其他用户。
- 成员点击"已扫码"的房主/副房主头像进行接力；未扫码的普通用户头像不可点。接力完成后可以退出房间。
- 每台设备底部显示房间在线用户与各自特征。只有房主与副房主可以扫码发布接力，普通成员只能接力；普通用户可以向房主申请副房主权限。
- 房间不独占设备：房主/副房主可以挂起当前房间、去加入其他房间，成员同理。
- 设备可以把数据重建为二维码，二维码下方显示内容；多枚并排，超过 3–5 枚或超出屏宽时换行。
- 物理位置与逻辑房间分离：房主可能在"房间 A"里，而加入同一逻辑房间的成员身处其他房间；接力正是为这种分离而存在。
- 测试在项目内进行：`tools/qr_bench.html` 是可用的双码手工测试台（长度、模块尺寸、纠错、字符集、拖拽、旋转、反色、净场）；测试分层为 fuzz / smoke / unit / unify，覆盖困难角度码、通信与插件。

## Capabilities and Constraints

**硬指标**

- zxing-cpp 快路径 30fps+（单帧 <33ms）；微信 CNN 分层路径允许放宽到 ≤60ms，且只在 fast miss 时触发。
- 多码同扫，尽量并行解码；取景不阻塞。
- 帧不能进 Dart：采集、推理、解码都留在原生层。

**扫码后端**

- 自实现 WeChat 式三段流水线：CNN 检测、按需超分、解码。`tools/qr_bench.html` 要分别给出"只检测+解码"与"检测+超分+解码"的耗时分布。
- 超分按需触发：例如检测框面积低于阈值才启用，禁止每帧无条件跑。
- 快路径与 WeChat crop 解码统一用 zxing-cpp Rust binding（vendored 0.5.3，Apache-2.0，bundled C++）；Android 使用预编译 `libZXing.so`，桌面用 bundled C++。二维码生成用纯 Rust `qrcode` crate。不使用 rxing/wxing。
- 推理后端已由实测决定：Android 用 `ort` + ONNX Runtime 1.31（真机 CNN 6–48ms），桌面用 tract 兜底。`st-zrt` 只是 ORT 封装（同内核，未单独重复测速）；`lele` 无 ONNX 前端，不适用；Candle 未实测——ORT 已在 60ms 预算内，先不引入第二套运行时。
- 参考权重是 WeChat 开源的 Caffe 格式（`detect` 检测、`sr` 超分，见 `WeChatCV/opencv_3rdparty`）。Apache-2.0，含 zxing 的 Apache-2.0 归属，商用可用，需保留许可证与归属声明；走 ONNX 后端时要处理 Caffe → ONNX 的转换，或自带解析。

**工程结构**

- 采集与推理按平台分层：capture-core / capture-android / capture-apple / capture-desktop / capture-wasm / capture 聚合层；CNN 后端同法分层。
- Web（wasm）客户端承担实时采集扫码，与原生端同功能，不是只做展示与接力。
- Rust workspace：`crates/` 下分 crate；最外层 `src/` 是唯一的 server 二进制。
- 可扩展但不做 Python 模组：保留 Rust trait、领域事件、协议与客户端导航等扩展点；移除 RustPython 运行时、模组上传/选择 UI 与 mod 产品承诺。
- 松耦合是硬要求：层与 crate 之间按能力切分接口，平台细节不得泄漏到上层；采集、推理、解码、分发各自可替换、可单独测试。
- TDD 开发；测试要保持通过率与速度，避免拖慢迭代的全量重跑。

**未定（未来工作不得当作已确认）**

- 产品正式名称：仓库与工作名是 `qr_race`，尚无品牌名、logo、配色或视觉识别。


## Evidence on Hand

- `PROMPT.md`：客户原始需求（中文），本记录的唯一权威来源。按上下文，"马中内容 / 马力扫描"应读作"码中内容 / 码力扫描"；这是阅读推断，未经客户书面确认。
- `tools/qr_bench.html`：可用的双码手工测试台，两枚码各自可调长度、模块尺寸、纠错等级与字符集，可拖拽、旋转、反色，并可切到净场只留白底与二维码；编码器从 CDN 取，首次打开需要网络。
- `.refs/opencv_3rdparty`：WeChat 开源的 Caffe 检测/超分权重与 LICENSE（gitignored 的参考副本）。
- `.refs/wxscan`：仅作 Flutter 插件形态参考；wxscan/wxing/TFLite 代码已删除，不再作为依赖或内部解码器。
- `G:\Projects\rcrm`：工程结构与工艺参考（crate 分层、Flutter 桥、脚本、文档体系），不是本产品的产品事实。
- 2026-10-20 zxing-cpp 合成套件（Windows x64、release、1280×720）：clean p50 10.4ms/p95 11.2ms；rotate 25.2ms；multi 29.6ms；camera_like 15.9ms。均为单线程整帧、未接 CNN，Android 彩色相机链路需重测。
- 2026-10-20 实机采集（Windows x64、集成摄像头、release）：相机权限开放后，真实纹理帧 1080p 全帧扫描平均 6.01ms（86 帧输入/60 扫描，3.586s），1280×720 平均 2.89ms（111 帧输入/60 扫描，3.915s）；两次都没有二维码入镜。更早相机返回 Windows 隐私占位帧时的 4.23ms（1080p）/1.95ms（720p）只是无特征帧的地板。录制帧率约 24–28fps，30fps 仍需更强的采集源。注意：42ms/19.5ms 来自 debug 构建，已作废。详见 `docs/PERF.md`。
- 2026-10-20 粗到精扫描（rxing 时代，release，1280×720 场景）：整帧解码 p50 4.629ms；金字塔检测 2.331ms；按需双线性超分 4.037ms。zxing-cpp 迁移后整帧基线见上；检测/超分仍是 trait 实现，CNN 已按同一层级接入（Android ORT）。
- 2026-10-20 wasm 与浏览器：wasm 扫描内核在 Node 中 1280×720 约 18–22ms；Edge headless + 假摄像头（Y4M）跑通 `getUserMedia → canvas → wasm`，640×480 命中，扫描约 1.8ms。物理相机实拍仍未验证。
- 2026-10-20 二维码生成：纯 Rust `qrcode` 编码 2048×2048 无损 WebP（纠错 M、quality 90 / method 4），与 rcrm bridge 参数一致；桌面 FFI 与单测验证 2048² 与回读解码。Web 走 vendored zxing-wasm（reader/writer），扫码与 2048² WebP 均已验证。Flutter 用 `Image.memory` 显示，已移除 `qr_flutter`。
- 2026-10-20 CNN 权重转换：自行解析 WeChat Caffe 权重并写 ONNX。SR 输入 1×1×224²、输出 1×1×447²，与独立 NumPy 前向最大误差 5.5e-06；检测器输入 1×1×384²，输出 mbox_loc/conf/priorbox（6912 priors），DetectionOutput 无 ONNX 等价物，decode/NMS 已在 `qr_race_wechat` 实现；Android ORT 已接入扫描管线。
- 2026-10-20 构建与测试：Rust 134 个测试、Flutter 27 个测试；Windows release、Web release、Android debug APK 均构建成功。
- 2026-10-20 Android 自启动与扫码：`qr_race_embedded` 编进 `libqr_race_ffi.so`，App 内可选自签名/已有证书/HTTP，默认端口 18443；真机启动后显示 URL、证书指纹与服务器账号，并通过指纹固定 + token 登录。Camera2 的 Y 平面经 JNI 直接进 Rust（旋转元数据、预览小图旋转；640×480 约 2–3ms、960×720 约 7.8ms、1280×720 约 6–16ms），实拍 `https://example.test/camera-hit` 命中；预览保留上一帧，不再黑屏闪烁。
- 2026-10-20 自动发布与动态码：扫码命中即自动发布，无需手动按钮；同一前缀（`A:`、`cam1|`）的新内容替换房间槽位，不同前缀可同时存在，房间按前缀显示最新一条。连接页支持 `qrrace://connect?...` 深链/粘贴链接预填服务器、指纹和 token；扫码/房间结果可一键跳微信、企业微信、浏览器或其他已安装 App，失败自动复制链接。
- 2026-10-20 每房间动态成员：服务端新增 host-only `/v1/room/guest`，房主进房后现场生成临时成员 token；邀请链接带房间地址，guest 直接登录并进房。实测 guest 以 member 身份加入，host 列表正确；不再依赖预置“成员账号”。
- 2026-10-20 桌面自部署进 bridge：Rust bridge 在 Windows Job Object 中启动 `qr_race_server`，GUI 关闭/崩溃会杀掉子进程并释放端口；配置/账号/SQLite 复用 `%APPDATA%\qr_race\selfhost\desktop`。桌面字体按 rcrm 加 `Microsoft YaHei/PingFang SC/SimHei/Noto Sans CJK SC` 回退。TOTP 2FA 已从 PRODUCT/PROMPT/UI/API 流程移除（只保留 legacy 校验代码）。
- 2026-10-20 自实现微信 CNN 分层流水线：`qr_race_wechat` 不再依赖 registry `wxscan`，算法模块与 ONNX 后端在本仓库（Android ORT / 桌面 tract）；zxing-cpp 全帧 fast path（彩色 RGB 优先，miss 先 try_harder 彩色重试），仍 miss 后 CNN 384 → 全分辨率 crop（zxing-cpp）→ 小码 SR → 每第 4 次 CNN 做 2×2 重叠分块检测。旧 rxing 真机数据：p95 fast 17.4ms、CNN 59.1ms；切到 zxing-cpp + YUV 彩色后待真机重测。`crates/cvlite` 仍 vendored 供 WeChat 缩放；wxscan/wxing 已删除。
- 2026-10-20 Ubuntu 中心服务器：新增 `scripts/deploy-server-linux.sh`、`qr_race_server fingerprint --cert`，systemd/前台两种模式；WSL Linux 实测自签名 HTTPS 启动、指纹与日志/CLI 一致、token-only 认证 200。自签名连接链接构造时必须带 64-hex 指纹。
- 2026-10-20 Android 真机：OnePlus PLF110（`3B163Q00LSK00000`）安装 debug APK；`adb reverse` + 自签名 HTTPS + 证书指纹 + token 登录成功进入大厅；房间里的接力二维码由 zxing-cpp WebP 正常显示。扫码页会弹出系统相机权限；拒绝后提供“重新申请/打开系统设置”，`MainActivity` 把不可再询问的拒绝映射为 permanentlyDenied 并跳转应用设置。相机采集已接入彩色与移动端 2K 请求（2560×1440，按设备能力回退）：Camera2 Y/U/V 三平面经 JNI 进 Rust，Rust 转 RGB 供 zxing-cpp（同时保留 luma 给 CNN/预览）；预览 1280px JPEG，扫码页全屏取景，识别气泡按四角坐标显示内容与广播状态。扫码后端可在混合/仅 zxing-cpp/微信 CNN 间切换。
- 2026-10-20 运维能力：SQLite 重启恢复、内置自签名 HTTPS（证书指纹固定）、登录失败 5 次锁定 60 秒、房间/角色/接力持久化均有过真实进程冒烟。
- 没有真实活动数据、用户研究、性能实测数字、证言、客户案例或视觉资产。未来页面与文档不得编造这些。

## Product Principles

1. **相机永不等待。** fast path 按 30fps 预算；CNN 层放宽到 60ms，昂贵的 SR/二次解码/网络按需触发，不能成为默认。
2. **一次扫描，全房间可见。** 扫到的码是房间里的接力件，不是日志里的一行字；分发速度与取景流畅度同等重要。
3. **房间是真实地点。** 京、区、级、房对应现实场地，房间码是鉴权路径的一部分；成员可以在别处，接力让这种分离成立。
4. **原生深度，薄接缝。** 帧与推理留在原生层，Flutter 只负责界面；模块在 server 与 client 两端都可拔插，接口要窄、可替换。
5. **生产即标准。** TDD、可复现的基准、明确的许可与安全边界；不拿演示级实现充数。





















