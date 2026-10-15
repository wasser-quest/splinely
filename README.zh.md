# Splinely

[English](README.md) · [Deutsch](README.de.md) · [Español](README.es.md) · [Français](README.fr.md) · [日本語](README.ja.md) · **简体中文**

![Splinely 标志](assets/splinely-logo.svg)

Splinely 是一款用 Rust 编写的桌面编辑器,使用 **`.sply` 项目文件**,并内置用于交互式 SVG 的 JavaScript 运行时。你可以绘制形状和路径,用关键帧、骨骼和状态机制作动画,最后导出为一个独立的 SVG 文件。**当前状态:可用的原型,尚未实现 [PLAN.md](PLAN.md) 中的全部功能。**

编辑器支持**英语、德语、西班牙语、法语、日语和简体中文**。默认跟随系统语言,可在 **设置 › 常规 › 语言** 中切换。日语和中文需要安装 CJK 字体(例如 Noto Sans CJK)。`docs/` 中的详细文档为德语。

## 用 LLM 安装

只需对一个能使用终端的 AI 代理(例如 Claude Code)说:

> 帮我安装 Splinely:https://raw.githubusercontent.com/wasser-quest/splinely/main/llms.txt

[`llms.txt`](llms.txt) 说明了代理应如何从发布页下载合适的预编译包、校验校验和、解压到 `~/.local/opt`,然后使用 `splinely-cli` 工作。

## 快速开始

**无需编译:** 每个 [发布版本](https://github.com/wasser-quest/splinely/releases/latest) 都附带 Linux 预编译包(`splinely-vX.Y.Z-x86_64-linux.tar.gz`,包含编辑器、CLI、查看器、运行时和 `libsplinely_runtime.so`)。安装说明:[docs/INSTALL-BINARIES.md](docs/INSTALL-BINARIES.md)。

从源码构建:

要求:Rust/Cargo;桌面窗口需要可用的图形/窗口环境。首次构建会下载依赖。Node.js 和 Python 3 仅用于检查脚本(`scripts/check.sh`)。

在 Linux 上构建编辑器和查看器需要 GTK/WebKit、ALSA 和 D-Bus 的开发包:

```sh
# Fedora
sudo dnf install gtk3-devel webkit2gtk4.1-devel libsoup3-devel alsa-lib-devel dbus-devel libxkbcommon-devel
# Debian/Ubuntu
sudo apt install libgtk-3-dev libwebkit2gtk-4.1-dev libsoup-3.0-dev libasound2-dev libdbus-1-dev libxkbcommon-dev pkg-config
```

```sh
git clone https://github.com/wasser-quest/splinely.git
cd splinely
cargo run -p splinely_editor --bin splinely
# 不打开窗口直接导出:
cargo run -p splinely_editor --bin splinely -- --demo-export /tmp/splinely-demo.svg
```

编辑器启动时会打开自带的演示。可以**拖入 `.sply` 文件**,使用 **文件 → 打开 …**(Ctrl+O,系统文件对话框)或 **最近打开**,也可以直接启动:`cargo run -p splinely_editor --bin splinely -- examples/demo.sply`。切换项目或关闭窗口前,会询问是否保留未保存的更改和绘图/源码草稿;窗口标题显示文件名,未保存时显示 `•`。**保存** 会保留可编辑的项目;SVG 只是可选的导出格式。

**设计 / 动画 / 高级** 将绘图、动画和代码分开。选择、钢笔、路径节点和手绘是互斥的工具;额外选项默认折叠。**预览**、**播放** 和 **打开 Splinely Viewer** 随时可用。[操作、手势与限制](docs/usability.md)。

自定义 JavaScript 只有在 **高级 → JavaScript** 中按会话明确允许后才会运行,绝不会在本地画布中运行。在 **动画 → 音频** 中可以开启基于 `ffplay` 的全局音频提示有限预览;准备工作在后台进行。

## 一个项目文件:`.sply`

几何形状、图层、骨骼、关键帧、状态、混合权重、变量、嵌入的音频和自定义代码都**保存在同一个文件中**。不需要额外的 JSON/SVG 或资源目录。`.sply` 内部是经过严格校验、可读的 JSON;旧的 `.json`/`.splinely.json` 项目仍然兼容。应用程序/运行时和已安装的编辑器插件不属于项目文件。[格式与兼容性](docs/sply-format.md)。

在 **动画** 中,**轻松制作动画**(关键帧/预设)和 **引导式状态机**(单击/悬停、可见性、片段开始)会带你完成最初的步骤。加权混合和定时淡化与专家工具并列可用。[分步说明与混合限制](docs/animation-workflow.md)。

```sh
splinely-cli new --demo --output scene.sply
splinely-cli convert legacy.splinely.json --output scene.sply
splinely-viewer scene.sply
# 仅用于已审查的自有代码:
splinely-viewer examples/desert-stickfight.sply --trusted-js
```

## Splinely Viewer

`cargo build -p splinely_viewer` 会构建真正的原生 SVG WebView。在编辑器中,**▶ Splinely Viewer** 会打开当前项目的快照,包括运行时、状态和嵌入的音频;无声的 resvg 画布与之分开。自定义 JS 需在 高级 中允许后才会运行。无需先导出 SVG 即可直接打开:`cargo run -p splinely_viewer --bin splinely-viewer -- examples/brown-owl.sply --trusted-js`(请先审查附带的代码)。已有的 SVG 同样支持。来自第三方的活动 SVG 不可信。Linux 需要 GTK3/WebKitGTK;[详情与测试](crates/splinely_viewer/README.md)。

导出的 SVG 包含 **Created with Splinely**、应用/架构版本以及机器可读的生成器元数据(`urn:splinely:metadata`)。没有可见水印;该标注可编辑,并不能证明真实性。

## 使用运行时

[运行时手册](docs/runtime-cookbook.md):播放/暂停/重新开始、片段时钟与全局时钟、HTML 嵌入、音频与故障排查。`splinely.snapshot()` 以复制的 JSON 数据返回运行时状态;`splinely.reportError(error)` 使用带有限历史的统一错误通道。

## 独立于浏览器的运行时

`splinely_runtime` 将 Splinely 文档核心以原生 Rust 运行时、C ABI(`cdylib`)和 `splinely-runtime` JSON 行进程的形式提供。其他语言可以发送事件、控制时间/片段权重并读取计算后的节点/骨骼。绘制、点击检测和音频由宿主负责——不需要浏览器,但也不是通用的 SVG JavaScript 解释器。[API 与 Python 示例](docs/native-runtime.md)。

```sh
cargo build -p splinely_runtime
python3 examples/native-runtime/smoke.py --library target/debug/libsplinely_runtime.so --binary target/debug/splinely-runtime
```

## CLI 工作流

CLI 和编辑器共用同一个文档核心。CLI 输出为 JSON;错误以 JSON 形式写到 stderr(附带指示下一步的 `hint`),并返回失败的退出码。不需要任何 AI 服务、API 密钥或服务器。

**入门:** `splinely-cli guide` 展示带有经过测试的示例的简短流程。省略 `--output` 时,编辑命令会直接保存到输入文件;`check` 会找出常见问题(被遮挡的形状、永不播放的片段、难以阅读的文字 …)并给出可执行的修复;`batch` 以"全部成功或全部不做"的方式执行多个步骤;`clip` 管理片段;`on` 让形状变得可交互。

```sh
F=/tmp/scene.sply
cargo run -p splinely-cli -- new --output $F --width 800 --height 450 --background "#0b1d3a"
cargo run -p splinely-cli -- node add $F --kind circle --id sun --x 400 --y 420 --radius 60 --fill orange --pivot center
cargo run -p splinely-cli -- keyframe $F --clip sunrise --target sun --property y --time 3 --by -260 --easing ease-out --autoplay --loop
cargo run -p splinely-cli -- on $F --event click --target sun --set sun.fill=gold
cargo run -p splinely-cli -- check $F
cargo run -p splinely-cli -- render $F --time 1.5 --output /tmp/scene.png
```

使用 JSON 和合并补丁:

```sh
cargo run -p splinely-cli -- new --demo --output /tmp/draft.splinely.json
cargo run -p splinely-cli -- validate /tmp/draft.splinely.json
cargo run -p splinely-cli -- inspect /tmp/draft.splinely.json
cargo run -p splinely-cli -- evaluate /tmp/draft.splinely.json --time 0.5
printf '%s\n' '{"name":"My animation"}' > /tmp/splinely-patch.json
cargo run -p splinely-cli -- apply /tmp/draft.splinely.json --patch /tmp/splinely-patch.json --output /tmp/final.splinely.json
cargo run -p splinely-cli -- export /tmp/final.splinely.json --output /tmp/final.svg
```

`schema` 返回带完整示例的格式指南,而不是正式的 JSON Schema。`apply` 使用 RFC 7396 合并补丁:数组会被整体替换。`evaluate` 不执行状态动作、JS 或音频;未激活的片段不会播放。详情:[CLI.md](CLI.md)、[CORE_API.md](CORE_API.md)。给其他 LLM 的资料:[创作指南](LLM.md) 和 [可复制的提示词](prompts/splinely-author.md);`schema` 和 `capabilities` 会列出实际支持的功能。

## 完整的离线示例

- [examples/demo.splinely.json](examples/demo.splinely.json):可编辑项目,包含一个圆形、3 秒的关键帧移动、单击触发的状态切换,以及在**全局播放时间 1.5 秒**处嵌入的 WAV 提示音。
- [examples/demo.svg](examples/demo.svg):真实的 CLI 导出结果,包含一个经明确允许的简短 `customJs` 处理程序。单击圆形会解锁音频并从头播放移动和时间线;第一次单击时圆形变为橙色。
- 声音是 **0.4 秒、类似音节的合成音,不含说话内容**。PCM WAV,单声道,8 kHz,16 位;没有外部音频文件。

```sh
# 可复现:CLI new --demo → apply → validate → export --trusted-js
bash scripts/generate-demo.sh
# 默认导出:代码仅被保存;开始按钮无需用户代码即可解锁音频。
cargo run -p splinely-cli -- export examples/demo.splinely.json --output /tmp/demo-safe.svg
# 仅在审查过自己的代码之后:
cargo run -p splinely-cli -- export examples/demo.splinely.json --trusted-js --output /tmp/demo-trusted.svg
```

在浏览器中以 `file://` 直接打开 `demo.svg`,先点击 **Start audio / animation**,再点击圆形。不要用 `<img>` 嵌入:那样 SVG 脚本不会执行。没有脚本时会显示静态初始画面。浏览器/CSP 可能会阻止脚本或动态代码编译;音频需要真实的用户交互和受支持的解码器。

附带的运行时和示例文件不需要外部库或网络资源。这**不是沙箱**:被允许的用户代码可以发起网络请求和执行其他浏览器操作。默认情况下 `customJs` 只会被保留而不执行;`rawSvg` 只作为元数据保留。不要用 `--trusted-js` 导出或打开来历不明的项目。

## 矢量、图层与骨骼

- **图层:** 选择、顺序、可见性和编辑锁定。颜色和透明度在检查器中设置。
- **路径:** 双击路径边缘会插入锚点且不改变形状;Shift 对称联动贝塞尔控制柄,Shift+Alt 保持相邻长度。[详情](docs/path-editing.md)。既可编辑 SVG 路径语法,也可在画面上编辑锚点/贝塞尔控制点;**钢笔** 通过单击和拖动绘制(Enter 完成,Escape 取消)。**Snap 10** 在松开时将位置局部吸附。矩形/圆形可以转换为路径,未修改时还可以还原。SVG 导入刻意做了限制,会拒绝不支持的外观,而不是悄悄丢弃。
- **骨骼:** 选择/移动关节,编辑父子关系和绑定,为旋转设置关键帧。刚性绑定,以及对无偏移的直接子关节进行有限的双骨骼 IK;不支持网格变形。加载 `examples/rig.splinely.json`;`examples/rig.svg` 会在浏览器中展示同样的动画。
- **导航:** F/Home 适应整个场景;+/- 缩放,0 恢复 100 %。中键或空格+左键平移视图而不改动形状;不拖动的空格在松开时开始/停止播放。标尺可以显示/隐藏,平移/缩放时仍保持文档坐标。时间轴可以隐藏。**预览** 将场景事件与编辑分开。
- **手绘/数位板:** 手绘模式生成可编辑的 SVG 路径,每一笔对应一步撤销。仅在有压力事件时支持压感;不对所有硬件/驱动作保证。[数位板说明](docs/tablet.md)。
- **即放即用插件:** 将 `splinely-plugin.json` 放入插件目录并重启,声明式的形状工具和调色板会自动加载,无需编译。[格式、目录与示例](docs/drop-in-plugins.md);绝不会自动执行第三方脚本/库。
- **插件:** 带有自身资源和装饰阶段的静态 Rust ECS 扩展。[模板](plugin-templates/editor/README.md);`splinely-cli plugin` 只生成源码,从不执行。原生插件属于受信任的代码,不是沙箱。

```sh
cargo run -p splinely-cli -- import-svg drawing.svg --output drawing.splinely.json
cargo run -p splinely-cli -- rig-demo --output /tmp/rig.splinely.json
cargo run -p splinely-cli -- export /tmp/rig.splinely.json --output /tmp/rig.svg
```

用 CLI 创建并排列形状,再以 PNG 检查(与编辑器逻辑相同):

```sh
cargo run -p splinely-cli -- node add scene.sply --kind circle --x 200 --y 150 --radius 40 --fill tomato --output scene.sply
cargo run -p splinely-cli -- arrange align scene.sply --nodes node-1,node-2 --edge top --output scene.sply
cargo run -p splinely-cli -- render scene.sply --output scene.png --width 800
```

更多形状、排列、骨骼、图层和转换命令:[CLI.md](CLI.md)。`keyframe` 可以单独添加节点/骨骼关键帧,无需完整的数组补丁。

## 已实现与待完成

| 已实现 | 未实现 / 推迟 |
| --- | --- |
| 矩形、圆形、文本、路径、多边形/星形/直线;多选和框选、缩放/旋转控制柄、智能参考线与手动参考线、右键菜单、命令面板(Ctrl+K)、可自定义快捷键的设置(Ctrl+,)、浅色/深色主题、界面语言、可见网格、录制模式(自动关键帧)、历史记录列表、指针旁的尺寸显示、带可拖动关键帧和缓动菜单的可视时间轴、可配置的洋葱皮、在画布上直接编辑文本、文档颜色、新项目预设、PNG/GIF 导出、吸管、线性/径向渐变、嵌套编组(树状视图、`<g>` 导出)、文本转路径、对齐/分布/翻转、保留曲线的布尔运算、路径变形、渐变动画、图层顺序、复制/粘贴、颜色/透明度、转换、撤销/重做 | 不同拓扑结构形状之间的变形为近似处理(子路径生长/收缩),不做精确的孔洞匹配 |
| 通过临时文件+重命名加载/保存项目;已修改项目每 30 秒自动保存并提供恢复对话框 | 历史架构迁移、完整的资源配额 |
| 时间轴、关键帧、缓动、独立的片段时钟、并行状态、加权片段和定时交叉淡化(`blendClip`) | 叠加混合、可视化混合树/混合空间 |
| 状态模型、守卫/动作、可编辑的状态图、JSON 编辑和本地状态预览 | 自由拖拽连线、自动图布局 |
| 浏览器事件、片段/播放 API、嵌入的音频资源/提示 | 原生 JS 执行或有保证的音频延迟;本地音频预览仅通过 ffplay 支持全局提示 |
| 静态/交互式 SVG 导出、有限的 SVG 矢量导入、JS/原始 SVG 源码字段 | **不支持任意 SVG 的完整导入**;导入时会拒绝文本/图像/渐变/滤镜;单独的原始 SVG 只作为元数据 |
| Bevy ECS 投影、可注册的静态插件、骨骼与刚性绑定 | 动态原生插件、网格蒙皮、通用 IK 链、将任意 Bevy 系统自动转换为浏览器代码、隔离的不受信任预览 |

本地预览使用 resvg 渲染 SVG:路径、圆角和变换都可见。文本使用本地系统字体,不同浏览器中可能略有差异。形状按精确轮廓选取;不可见的形状(不透明度为 0)无法点击。架构 1 已扩展;旧项目加载时新字段使用默认值。旧版 Splinely 无法识别新字段。`.sply` 项目文件(内部为 JSON)是唯一权威来源;SVG 导出不能无损地往返导入。

## 检查

```sh
bash scripts/check.sh
```

该脚本会运行工作区测试、Node 运行时测试、真实的 CLI 演示生成/计算/导出,以及基于 Python 的 XML/元数据/WAV/资源检查,并检查附带的示例文件。**它不能替代真实的浏览器/音频测试。** 此外,`python3 scripts/svg-corpus.py` 会检查已安装的真实 SVG,`python3 scripts/browser-smoke.py examples/demo.svg /tmp/splinely-svg-corpus/manifest.json` 会在 Firefox 中测试并进行像素对比。测试涵盖严格 JSON、路径、图层、骨骼、插件生命周期、运行时安全回归以及每种界面语言。实际的声音输出和音频延迟尚未验证。复现与手动验收:[docs/browser-tests.md](docs/browser-tests.md)。原生 GUI 截图与产物检查:[docs/verification.md](docs/verification.md)。

## 动画展示

[展示总览](examples/showcase.html) · [交互式 SVG、直接事件与不可见状态](docs/interactive-svg.md)

- **[Sand / Accord](examples/desert-stickfight.html)**([可编辑的 `.sply`](examples/desert-stickfight.sply)):原创 20 秒短片,两个带骨骼的角色,采用 Cactus Runner 温暖的沙漠风格,五个对战阶段和友好的结局;播放/暂停/重播。[项目、生成与真实 Firefox 检查](docs/desert-stickfight.md)。
- [Cactus Runner](examples/cactus-runner.html):奔跑的骨骼角色、滚动的仙人掌和单击跳跃,真实混合行走/跳跃权重。[详情/检查](docs/cactus-runner.md)。
- [Cinder / Flux – Stickfight](examples/stickfight.html):两个骨骼角色之间原创的 8 秒动画格斗,包含闪避/反击、阴影/火花以及播放/暂停/重新开始。`python3 scripts/generate-stickfight.py`,检查:`python3 scripts/stickfight-smoke.py`。
- [棕色猫头鹰](examples/brown-owl.html):树枝上的猫头鹰,带有直接的单击/悬停状态——悬停时眼睛跟随指针、耳羽竖起;单击时它会满足地闭上眼睛并抬起翅膀。检查:`python3 scripts/owl-smoke.py`。
- [Aoi 打招呼](examples/anime-hello.html):带骨骼手部的原创动漫风格角色。**本地生成的 WAV 嵌入在 SVG 中**,无需网络连接。单击解锁音频。复现:`python3 scripts/generate-anime-demo.py`(需要本地的 `espeak-ng`/`espeak`);浏览器检查:`python3 scripts/anime-smoke.py`。
- [蓝色球体上的水滴](examples/water-drop.html):独立的 SVG 场景,带有挤压/拉伸、高光和涟漪;播放/暂停。
- [随机图形](examples/random-figures.html):可通过种子复现,状态带字幕自动依次切换。点击 **Play**。状态切换跟随片段结束。
- 两个 HTML 文件都通过 `object` 嵌入相邻的 SVG,不使用服务器 API。测试服务器只是本地浏览器测试辅助工具。
- 生成:`splinely-cli water-drop --help`、`splinely-cli random-figures --help`;完整示例见 [CLI.md](CLI.md)。浏览器检查:`python3 scripts/showcase-smoke.py --duration 35`。

## 标志

原创 SVG 标志:用贝塞尔样条绘制的 S,带有锚点、控制柄和运动轨迹。

## 许可证

MIT,见 [LICENSE](LICENSE)。
