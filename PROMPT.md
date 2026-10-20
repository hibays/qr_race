参考 G:\Projects\rcrm 的项目结构，自实现[wechat_qrscan](https://github.com/opencv/opencv_contrib/tree/5.x/modules/wechat_qrcode)作为扫码后端，支持多码同扫，无阻塞，界面上显示检测成功反馈框，尽可能使用成熟、热度、现代、活跃的库和部分的自实现，网络搜索。

Target Platforms: Android	arm64-v8a, armeabi-v7a, x86_64；iOS，macOS，Linux, Windows；Web
硬要求：zxing-cpp 快路径 30fps+（单帧 <33ms，彩色相机）；微信 CNN 分层路径允许放宽到 ≤60ms，且必须按需触发
技术栈上追求高性能，混合原生，同时有wasm支持，通过脚本预设 features 或分 crate 实现各 target 不同后端同功能，用flutter实现多平台原生gui，注意各层粒度分布合理工程化。保留 Rust trait/事件/协议层的扩展点即可；不实现 Python 模组运行时，也不要求可拔插 server/client mod。
各crate应置于 crates/ 下，最外层 src/ 应为单一 server 二进制

https://github.com/wilinz/wxscan -- 插件 that 实现wechat_qrscan在flutter，可为参考
注：
其自实现的 cvlite 和 tflite 后端都不怎么样，扫码统一用 zxing-cpp；网络搜索 onnx 的高性能后端，按平台选择

TDD 开发，要注意测试项目通过率和速度，使用 fuzz/smoke/unit/unify 测试，测试项目可以参考 tools/qr_bench.html 和自构建，以及各角度困难码和通信、plugin等

---

MVP 工作路径如：
0. 用户ABCD获得 server url + auth token，在客户端输入连接到 server，server 端进行 token 验证，等待鉴权成功
1. 用户A手持设备1在客户端建立一个房间并进入，作为房主，可以设置多个副房主，单端扫码，并端侧解码后将数据压缩上传到 server-side
2. 用户BCD分别使用设备234通过客户端进入A创建的房间，从服务器获得房主扫描得到的数据
3. 各用户设备可以将数据转为二维码，二维码下方显示二维码内容，多个二维码可以并排，数量大于3-5（或大于屏宽）时多行，展示
4. 每个设备都可以看见当前房间用户有谁，在界面（底部）显示用户特征，每个用户都具有扫码和接力功能
5. 辅房主进行扫码时特化他的用户特征，其他用户点击其头像就可以进行接力，不能通过点击未扫码普通用户头像进行接力
6. 房主扫码后广播到所有其他用户界面，显示接力码和接力内容
7. 只有房主和副房主可以扫码（发布接力），普通用户可以向房主申请副房主权限
8. 其他用户接力完成后可以退出房间

## 什么是房间

房间是固定地理逻辑概念，分布在两京（东京，西京），每京下辖多区，每区有多级，每级有多房，如东京A区1B级103房（数字和字母 -- 1B），西京Z区B级103房等，位置和地理固定，并且每房都有一个固定对照识别代码可以用于构造有效鉴权路径。

用户建立房间时可以随机建立，也可以 active 现有 unactive 房间进行点亮，actived 的房间可以在大厅显示。

房主可以设置加入密码，也可以随时撤销。

## 什么是接力

接力是房主在房间向房间其他成员发起的房主主导的马力扫描，房主（或者副房主）地理房间扫码（可以挂起加入其他房间，成员同理）后向成员广播马中内容，并复构建出qr码，并可以成为其他房间房主对此接力

## 扩展性（非模组）

不实现 RustPython/Python 模组，也不把“可拔插 mod”作为产品能力。工程保留 Rust trait、
领域事件、传输协议和客户端导航等扩展点，后续若要接入新的扫码后端、采集平台或
服务端策略，可以在 crate 边界替换，而不是运行时加载脚本。

---

## 补充说明

- 不要用 rqrr
- 多码同扫应尽量并行解码
- 帧不能进 Dart
可以使用capture-core/        capture-android/   capture-apple/
  capture-desktop/     capture-wasm/      capture/ 这样的平台分层，包括wxscan的cnn模型后端也可以这样分层，具体你自行斟酌
- 要注意松耦合等的工程化设计
- 可以尝试在原生使用 st-zrt，或者 Candle 或 lele，当然可以分平台选择性能最优
- wechat_qrscan 应该通过运行时策略启用，在 qr_bench 中分别测“只检测+解码”和“检测+超分+解码”的耗时分布。快路径 30fps、CNN 层 ≤60ms 的预算下，超分应当按需触发（例如检测框面积低于阈值才启用），而不是每帧无条件跑。


