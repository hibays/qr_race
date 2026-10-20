# wxscan-ffi

[English](README.md) · **简体中文**

[`wxscan`](https://github.com/wilinz/wxscan-rs/tree/main/crates/wxscan) 的 C ABI，给 C、C++、Swift、Kotlin、Python 以及别的语言的调用方
用。

`include/wxscan.h` 由这些源码用 cbindgen 生成，并且提交进了仓库，所以使用方既不需要
cbindgen，也不需要 Rust 工具链。

<img src="https://raw.githubusercontent.com/wilinz/wxscan/main/docs/demo.webp" width="300"
     alt="一帧里两个二维码都被框出，点开其中一个显示解出的中文文本，按 UTF-8 读取。">

*录像里的一切都经过这套 ABI：相机帧从 Swift 直接进来，角点也原路回去。*

```c
#include "wxscan.h"

// 两个模型都可以传 NULL，那就是不带 CNN 阶段的纯解码模式。
// 权重加载失败时句柄为零。
WxScanScannerId scanner = wxscan_scanner_new(detect, detect_len, sr, sr_len);

// 一张正立的、紧密排列的灰度图。
WxScanResults *out = wxscan_scan_gray(scanner, gray, width, height);
for (size_t i = 0; i < out->results_len; i++) {
    printf("%s\n", out->results[i].text);
}
wxscan_results_free(out);

wxscan_scanner_release(scanner);
```

权重在磁盘上就改传路径，文件由本库来读，这样绑定层不需要自己的文件 API，一兆字节也
不用过它那道边界：

```c
WxScanStatus status;
WxScanScannerId scanner =
    wxscan_scanner_new_path("/var/wxscan/detect.tflite",
                            "/var/wxscan/sr.tflite", &status);
// 返回 0 时看 status，它说的是路径错在哪一种：BadArgument 是字符串不是 UTF-8，
// Unreadable 是文件打不开，WeightsRefused 是文件读到了但不是这个构建能加载的
// 权重。打错字、没下载、拿错文件，是三个不同的错，也在三个不同的地方修。
```

两个路径都可以传 NULL，意思是那个网络不存在，跟上面传 NULL 缓冲一样。这个入口在
`model-fs` feature 后面，默认开着。它只用到 `std::fs`，但 wasm 构建会跟其它默认项一起
把它关掉，而不是带上一个只可能失败的函数——浏览器没有文件系统可读。

磁盘上的图片交给 `wxscan_scan_path`，它自己读文件、自己解码，所以手上只有一个路径的
调用方永远不必把像素落地。一张 1200 万像素的照片按 RGBA 算是 48 MB，而把它跨线程或者
跨 isolate 传一遍的调用方，这块缓冲要付的代价不止一次。

```c
WxScanStatus status;
WxScanResults *out = wxscan_scan_path(scanner, "/tmp/photo.jpg", &status);
// status 分得清「文件读不了」和「图里没有码」；后者是 Ok，
// 只是结果集为空。
```

文件里记的方向会被应用上，所以横过来拍的照片扫的时候是正立的，坐标也和当时屏幕上看到
的对得上。这个入口在 `image-io` feature 后面，每个解码器还各有自己的 feature：`png`、
`jpeg`、`gif` 默认开着，相册选择器写得出来的就这几种，三个加起来 436 KB；`webp`、
`bmp`、`tiff` 另算 570 KB，默认关着——这几种不是相机写的，是从别处来的。HEIC 一个都
不在里面，它要的是 HEVC 解码器；非读不可的调用方用 `wxscan_set_image_decoder` 把平台
自己的借进来，或者自己解完调 `wxscan_scan_pixels`。压根用不上编码图片的构建把
`image-io` 也关掉，解码器和入口一起没有。

相机帧交给 `wxscan_scan_frame`，它还多收三样：行跨距、旋转，以及一个把 x 坐标镜像回去
的标志。帧本身从不镜像，因为检测器是在非镜像输入上训练的。这个标志是为了让坐标跟一个
镜像显示的预览对得上，前置摄像头的预览通常就是镜像的。

结果就是普通的 C 结构体。调用方要是需要序列化，那是它那层绑定的事。

扫描器是一个显式的句柄，不是全局单例，所以多个扫描器可以带着不同的模型共存，调用之间
也不会去争同一把锁。一个实例同一时刻扫一帧。

### 句柄不是指针

`WxScanScannerId` 是本库发放的一个数字，本库在自己的一张表里查它。已经释放的句柄、
从来没存在过的句柄、调用方凭空编的句柄，都查不到东西，于是按一次普通的失败返回：零、
NULL 结果、`WxScanStatus::BadArgument`。换成地址就会被解引用，崩溃还会落在离出错点很
远的地方。

句柄从不复用，所以陈旧的句柄绝不会在之后变成指向另一个扫描器。

它还是引用计数的。两边同时持有一个扫描器是常态，而且谁也看不见对方的生命周期：托管
应用为静态图片持着它，相机绑定同时拿同一个句柄解帧。

```c
wxscan_scanner_retain(scanner);   // 相机绑定现在也是持有者
...
wxscan_scanner_release(scanner);  // 然后还回去；最后一个走的人负责释放
```

只有单一持有者的调用方从不调 retain，而是像对待别的分配那样，把 `new` 和 `release`
配成一对。释放一个已经不存在的句柄什么也不做，debug 构建会在 stderr 上说一声。但在
另一个持有者还在的时候多释放一次，拿走的是**那个持有者的**引用，而且是静默的。这件事
检测不出来：两次释放彼此无法区分。取了几次引用，就释放几次。

## 链接

本 crate 编成静态库和 rlib。它有意不产出 cdylib：那会要求在构建时就解析 TFLite 的符号，
而这里的设计是把它们留给宿主构建系统的最终链接步骤。想要共享库的调用方，应该拿自己的
一个 crate 把本 crate 包起来，在那里提供搜索路径。
[wxscan](https://github.com/wilinz/wxscan) 里的 `wxscan_core` 包就是这么做的，覆盖了
五个平台，可以照抄。

[wxscan-rs](https://github.com/wilinz/wxscan-rs) 的一部分。Apache-2.0。
