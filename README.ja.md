# Splinely

[English](README.md) · [Deutsch](README.de.md) · [Español](README.es.md) · [Français](README.fr.md) · **日本語** · [简体中文](README.zh.md)

![Splinely ロゴ](assets/splinely-logo.svg)

Splinely は、**`.sply` プロジェクトファイル**とインタラクティブ SVG 用の JavaScript ランタイムを内蔵した Rust 製デスクトップエディターです。シェイプやパスを描き、キーフレーム・ボーン・ステートマシンでアニメーションを付け、1 つの自己完結した SVG として書き出せます。**現状: 動作するプロトタイプです。[PLAN.md](PLAN.md) の全機能はまだ揃っていません。**

エディターは**英語・ドイツ語・スペイン語・フランス語・日本語・簡体字中国語**に対応しています。既定ではシステムの言語に従い、**設定 › 一般 › 言語** で切り替えられます。日本語と中国語には CJK フォント (Noto Sans CJK など) が必要です。`docs/` の詳細ドキュメントはドイツ語です。

## LLM でインストール

シェルを使える AI エージェント (Claude Code など) に次のように伝えるだけです:

> Splinely をインストールして: https://raw.githubusercontent.com/wasser-quest/splinely/main/llms.txt

[`llms.txt`](llms.txt) には、リリースから適切なビルド済みパッケージをダウンロードし、チェックサムを確認して `~/.local/opt` に展開し、`splinely-cli` で作業する手順が書かれています。

## はじめに

**コンパイル不要:** 各 [リリース](https://github.com/wasser-quest/splinely/releases/latest) に Linux 用のビルド済みパッケージ (`splinely-vX.Y.Z-x86_64-linux.tar.gz`、エディター・CLI・ビューアー・ランタイム・`libsplinely_runtime.so` 入り) があります。インストール方法: [docs/INSTALL-BINARIES.md](docs/INSTALL-BINARIES.md)。

ソースからビルドする場合:

必要なもの: Rust/Cargo、デスクトップウィンドウ用のグラフィック/ウィンドウ環境。初回ビルドで依存関係がダウンロードされます。Node.js と Python 3 はチェックスクリプト (`scripts/check.sh`) にのみ必要です。

Linux でエディターとビューアーをビルドするには、GTK/WebKit・ALSA・D-Bus の開発パッケージが必要です:

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
# ウィンドウを開かずに書き出す:
cargo run -p splinely_editor --bin splinely -- --demo-export /tmp/splinely-demo.svg
```

エディターは内蔵デモで起動します。**`.sply` ファイルをドロップ**するか、**ファイル → 開く …** (Ctrl+O、システムのファイルダイアログ) や **最近使ったファイル** を使うか、直接起動します: `cargo run -p splinely_editor --bin splinely -- examples/demo.sply`。未保存の変更や描画/ソースの下書きは、切り替え時とウィンドウを閉じるときに確認されます。タイトルにはファイル名と、未保存時の `•` が表示されます。**保存** で編集可能なプロジェクトを残し、SVG は任意の書き出しです。

**デザイン / アニメート / 詳細** で描画・アニメーション・コードを分けています。選択・ペン・パスポイント・フリーハンドは排他的なツールで、追加オプションは折りたたまれています。**プレビュー**・**再生**・**Splinely Viewer を開く** はいつでも使えます。[操作・ジェスチャー・制限](docs/usability.md)。

独自の JavaScript は **詳細 → JavaScript** でセッションごとに明示的に許可した場合のみ実行され、ネイティブキャンバスでは決して実行されません。**アニメート → オーディオ** では、グローバルオーディオキューを `ffplay` で限定的にプレビューできます (準備はバックグラウンドで行われます)。

## 1 つのプロジェクトファイル: `.sply`

ジオメトリ、レイヤー、ボーン、キーフレーム、ステート、ブレンドウェイト、変数、埋め込みオーディオ、独自コードが**1 つのファイルにまとまります**。追加の JSON/SVG やアセットフォルダーは不要です。`.sply` の中身は厳密に検証された読みやすい JSON で、古い `.json`/`.splinely.json` プロジェクトも引き続き使えます。アプリ/ランタイムやインストール済みプラグインはプロジェクトファイルに含まれません。[形式と互換性](docs/sply-format.md)。

**アニメート** では **かんたんアニメーション** (キーフレーム/プリセット) と **ガイド付きステートマシン** (クリック/ホバー、表示、クリップ開始) が最初の手順を案内します。加重ブレンドと時間指定のフェードもエキスパートツールと並んで使えます。[手順とブレンドの制限](docs/animation-workflow.md)。

```sh
splinely-cli new --demo --output scene.sply
splinely-cli convert legacy.splinely.json --output scene.sply
splinely-viewer scene.sply
# 確認済みの独自コードの場合のみ:
splinely-viewer examples/desert-stickfight.sply --trusted-js
```

## Splinely Viewer

`cargo build -p splinely_viewer` で本物のネイティブ SVG WebView をビルドします。エディターの **▶ Splinely Viewer** は、ランタイム・ステート・埋め込みオーディオを含む現在のプロジェクトのスナップショットを開きます (無音の resvg キャンバスとは別です)。独自 JS は 詳細 で許可した後のみ実行されます。SVG を書き出さずに直接開くには: `cargo run -p splinely_viewer --bin splinely-viewer -- examples/brown-owl.sply --trusted-js` (同梱コードを先に確認してください)。既存の SVG も開けます。第三者のアクティブな SVG は信頼できません。Linux では GTK3/WebKitGTK が必要です。[詳細とテスト](crates/splinely_viewer/README.md)。

書き出した SVG には **Created with Splinely**、アプリ/スキーマのバージョン、機械可読なジェネレーターメタデータ (`urn:splinely:metadata`) が含まれます。目に見える透かしはありません。表記は編集可能で、真正性の証明ではありません。

## ランタイムの使い方

[ランタイム・クックブック](docs/runtime-cookbook.md): 再生/一時停止/再起動、クリップとグローバルの時計、HTML への埋め込み、オーディオ、トラブルシューティング。`splinely.snapshot()` はランタイムの状態を JSON データのコピーとして返し、`splinely.reportError(error)` は履歴付きの共通エラーチャネルを使います。

## ブラウザーに依存しないランタイム

`splinely_runtime` は Splinely のドキュメントコアを、ネイティブ Rust ランタイム、C ABI (`cdylib`)、`splinely-runtime` JSON-lines プロセスとして提供します。他の言語からイベントを送り、時間やクリップのウェイトを制御し、評価済みのノード/ボーンを取得できます。描画・当たり判定・オーディオはホスト側が担当します。ブラウザーは不要ですが、汎用の SVG JavaScript インタープリターでもありません。[API と Python の例](docs/native-runtime.md)。

```sh
cargo build -p splinely_runtime
python3 examples/native-runtime/smoke.py --library target/debug/libsplinely_runtime.so --binary target/debug/splinely-runtime
```

## CLI での作業

CLI とエディターは同じドキュメントコアを使います。CLI の結果は JSON で、エラーは stderr に JSON (次の手順を示す `hint` 付き) として出力され、失敗の終了コードを返します。AI プロバイダー、API キー、サーバーは不要です。

**はじめ方:** `splinely-cli guide` で、テスト済みのレシピ付きの短い手順を表示します。編集コマンドは `--output` を省略すると入力ファイルに保存し、`check` はよくある間違い (隠れたシェイプ、再生されないクリップ、読めないテキスト …) を実行可能な修正付きで見つけ、`batch` は複数の手順をすべて成功するか何もしないかで実行し、`clip` はクリップを管理し、`on` はシェイプをインタラクティブにします。

```sh
F=/tmp/scene.sply
cargo run -p splinely-cli -- new --output $F --width 800 --height 450 --background "#0b1d3a"
cargo run -p splinely-cli -- node add $F --kind circle --id sun --x 400 --y 420 --radius 60 --fill orange --pivot center
cargo run -p splinely-cli -- keyframe $F --clip sunrise --target sun --property y --time 3 --by -260 --easing ease-out --autoplay --loop
cargo run -p splinely-cli -- on $F --event click --target sun --set sun.fill=gold
cargo run -p splinely-cli -- check $F
cargo run -p splinely-cli -- render $F --time 1.5 --output /tmp/scene.png
```

JSON とマージパッチを使う場合:

```sh
cargo run -p splinely-cli -- new --demo --output /tmp/draft.splinely.json
cargo run -p splinely-cli -- validate /tmp/draft.splinely.json
cargo run -p splinely-cli -- inspect /tmp/draft.splinely.json
cargo run -p splinely-cli -- evaluate /tmp/draft.splinely.json --time 0.5
printf '%s\n' '{"name":"My animation"}' > /tmp/splinely-patch.json
cargo run -p splinely-cli -- apply /tmp/draft.splinely.json --patch /tmp/splinely-patch.json --output /tmp/final.splinely.json
cargo run -p splinely-cli -- export /tmp/final.splinely.json --output /tmp/final.svg
```

`schema` は完全な例付きのフォーマットガイドを返します (正式な JSON Schema ではありません)。`apply` は RFC 7396 のマージパッチを使い、配列は丸ごと置き換えられます。`evaluate` はステートアクション・JS・オーディオを実行せず、有効でないクリップは再生しません。詳細: [CLI.md](CLI.md)、[CORE_API.md](CORE_API.md)。他の LLM 向け: [作成ガイド](LLM.md) と [コピー用プロンプト](prompts/splinely-author.md)。実際に対応している機能は `schema` と `capabilities` で確認できます。

## オフラインで完結する例

- [examples/demo.splinely.json](examples/demo.splinely.json): 円、3 秒間のキーフレーム移動、クリックによるステート変更、そして**グローバル再生時間 1.5 秒**の埋め込み WAV キューを含む編集可能なプロジェクト。
- [examples/demo.svg](examples/demo.svg): 明示的に許可した短い `customJs` ハンドラー付きの実際の CLI 書き出し。円をクリックするとオーディオが有効になり、移動と再生が最初から始まります。最初のクリックで円がオレンジになります。
- 音は**話し言葉ではない、0.4 秒の音節のような合成音**です。PCM WAV、モノラル、8 kHz、16 ビット。外部のオーディオファイルはありません。

```sh
# 再現可能: CLI new --demo → apply → validate → export --trusted-js
bash scripts/generate-demo.sh
# 既定の書き出し: コードは保存のみ。開始ボタンがユーザーコードなしでオーディオを有効にします。
cargo run -p splinely-cli -- export examples/demo.splinely.json --output /tmp/demo-safe.svg
# 自分のコードを確認した後のみ:
cargo run -p splinely-cli -- export examples/demo.splinely.json --trusted-js --output /tmp/demo-trusted.svg
```

`demo.svg` をブラウザーで `file://` として直接開き、**Start audio / animation**、続いて円をクリックします。`<img>` として埋め込まないでください (SVG のスクリプトが実行されません)。スクリプトが無効なら静的な初期画像が表示されます。ブラウザーや CSP はスクリプトや動的なコードのコンパイルをブロックすることがあり、オーディオには実際のユーザー操作と対応デコーダーが必要です。

同梱のランタイムとサンプルファイルは、外部ライブラリーやネットワークリソースを必要としません。これは**サンドボックスではありません**: 許可したユーザーコードはネットワーク通信などのブラウザー操作を行えます。既定では `customJs` は保持されるだけで実行されず、`rawSvg` はメタデータのままです。素性の分からないプロジェクトを `--trusted-js` で書き出したり開いたりしないでください。

## ベクター、レイヤー、ボーン

- **レイヤー:** 選択、順序、表示、編集ロック。色と透明度はインスペクターで設定します。
- **パス:** パスの辺をダブルクリックすると形を変えずにアンカーを挿入します。Shift でベジェハンドルを対称に連動、Shift+Alt で隣の長さを維持します。[詳細](docs/path-editing.md)。SVG のパス構文と、画面上のアンカー/ベジェ制御点の両方を編集できます。**ペン** はクリックとドラッグで描きます (Enter で完了、Escape でキャンセル)。**Snap 10** は離したときに位置をローカルにスナップします。長方形/円をパスに変換し、未変更なら元に戻せます。SVG の読み込みは意図的に限定されており、未対応の見た目は黙って失うのではなく拒否します。
- **ボーン:** 関節の選択/移動、親とバインドの編集、回転のキーフレーム。剛体バインドと、オフセットのない直接の子関節に対する限定的な 2 ボーン IK。メッシュ変形はありません。`examples/rig.splinely.json` を読み込むと、`examples/rig.svg` で同じアニメーションをブラウザーで確認できます。
- **ナビゲーション:** F/Home でシーン全体を表示、+/- でズーム、0 で 100 %。中ボタンまたはスペース+左ボタンでシェイプを変えずに表示を移動します。ドラッグしないスペースは離したときに再生/停止します。定規は表示/非表示を切り替えられ、パン/ズーム中もドキュメント座標に沿います。タイムラインは非表示にできます。**プレビュー** はシーンイベントと編集を分けます。
- **フリーハンド/タブレット:** フリーハンドモードは編集可能な SVG パスを作り、1 ストロークが 1 回の取り消しになります。筆圧はフォースイベントがある場合のみで、ハードウェアやドライバーを全面的に保証するものではありません。[タブレットの注意](docs/tablet.md)。
- **ドロップインプラグイン:** `splinely-plugin.json` をプラグインフォルダーに置いて再起動すると、宣言的なシェイプツールとパレットがコンパイルなしで読み込まれます。[形式・フォルダー・例](docs/drop-in-plugins.md)。第三者のスクリプト/ライブラリーが自動で実行されることはありません。
- **プラグイン:** 独自リソースとデコレートフェーズを持つ静的な Rust ECS 拡張。[テンプレート](plugin-templates/editor/README.md)。`splinely-cli plugin` はソースを生成しますが実行はしません。ネイティブプラグインは信頼されたコードであり、サンドボックスではありません。

```sh
cargo run -p splinely-cli -- import-svg drawing.svg --output drawing.splinely.json
cargo run -p splinely-cli -- rig-demo --output /tmp/rig.splinely.json
cargo run -p splinely-cli -- export /tmp/rig.splinely.json --output /tmp/rig.svg
```

CLI でシェイプを作成・整列し、PNG で確認します (エディターと同じロジック):

```sh
cargo run -p splinely-cli -- node add scene.sply --kind circle --x 200 --y 150 --radius 40 --fill tomato --output scene.sply
cargo run -p splinely-cli -- arrange align scene.sply --nodes node-1,node-2 --edge top --output scene.sply
cargo run -p splinely-cli -- render scene.sply --output scene.png --width 800
```

シェイプ、整列、ボーン、レイヤー、変換のその他のコマンド: [CLI.md](CLI.md)。`keyframe` は配列全体のパッチなしで、ノード/ボーンのキーを 1 つずつ追加します。

## 実装済みと未実装

| 実装済み | 未実装 / 保留 |
| --- | --- |
| 長方形、円、テキスト、パス、多角形/星/直線。複数選択と矩形選択、拡大縮小/回転ハンドル、スマートガイドと手動ガイド、コンテキストメニュー、コマンドパレット (Ctrl+K)、ショートカットを自由に割り当てられる設定 (Ctrl+,)、ライト/ダークテーマ、UI 言語、グリッド表示、記録モード (自動キー)、履歴、ポインター横の寸法表示、ドラッグ可能なキーフレームと補間メニューを持つタイムライン、設定可能なオニオンスキン、キャンバス上でのテキスト編集、ドキュメントの色、新規プロジェクトのプリセット、PNG/GIF 書き出し、スポイト、線形/放射状グラデーション、入れ子のグループ (ツリー表示、`<g>` 書き出し)、テキストのパス変換、整列/分布/反転、曲線を保つブーリアン演算、パスモーフ、グラデーションのアニメーション、レイヤー順、コピー/貼り付け、色/透明度、変換、取り消し/やり直し | トポロジーの異なるシェイプ間のモーフは近似 (部分パスの伸縮) で、穴の正確な対応付けはありません |
| 一時ファイル+リネームによるプロジェクトの読み込み/保存、変更されたプロジェクトの 30 秒ごとの自動保存と復元ダイアログ | 過去のスキーマからの移行、完全なリソース上限 |
| タイムライン、キーフレーム、イージング、独立したクリップの時計、並列ステート、加重クリップと時間指定のクロスフェード (`blendClip`) | 加算ブレンド、視覚的なブレンドツリー/ブレンドスペース |
| ステートモデル、ガード/アクション、編集可能なステートグラフ、JSON 編集、ネイティブのステートプレビュー | 自由なドラッグ接続、グラフの自動レイアウト |
| ブラウザーイベント、クリップ/再生 API、埋め込みオーディオアセット/キュー | ネイティブでの JS 実行や保証されたオーディオ遅延。ネイティブのオーディオプレビューは ffplay によるグローバルキューのみ |
| 静的/インタラクティブ SVG 書き出し、限定的な SVG ベクター読み込み、JS/生 SVG ソース欄 | **任意の SVG の完全な読み込みには非対応**。テキスト/画像/グラデーション/フィルターは読み込み時に拒否。生 SVG 単体はメタデータのまま |
| Bevy ECS プロジェクション、登録可能な静的プラグイン、ボーンと剛体バインド | 動的ネイティブプラグイン、メッシュスキニング、汎用 IK チェーン、任意の Bevy システムのブラウザーコードへの自動変換、隔離された非信頼プレビュー |

ネイティブプレビューは resvg で SVG を描画し、パス・角丸・変形が表示されます。テキストはローカルのシステムフォントを使うため、ブラウザーによって異なることがあります。シェイプは正確な輪郭で選択され、不可視のシェイプ (不透明度 0) はクリックできません。スキーマ 1 は拡張されており、古いプロジェクトは新しい項目に既定値を使って読み込まれます。古い Splinely は新しい項目を理解しません。基準となるのは `.sply` プロジェクトファイル (中身は JSON) で、SVG 書き出しは無損失の往復ではありません。

## チェック

```sh
bash scripts/check.sh
```

このスクリプトはワークスペースのテスト、Node のランタイムテスト、CLI による実際のデモ生成/評価/書き出し、Python による XML/メタデータ/WAV/リソースのチェックを行い、同梱のサンプルファイルも確認します。**実際のブラウザー/オーディオのテストの代わりにはなりません。** さらに `python3 scripts/svg-corpus.py` はインストール済みの実在 SVG を、`python3 scripts/browser-smoke.py examples/demo.svg /tmp/splinely-svg-corpus/manifest.json` はピクセル比較を含めて Firefox をテストします。テストは厳密な JSON、パス、レイヤー、ボーン、プラグインのライフサイクル、ランタイムのセキュリティ回帰、すべての UI 言語を対象にしています。実際の音声出力と遅延は未検証です。再現と手動の受け入れ確認: [docs/browser-tests.md](docs/browser-tests.md)。ネイティブ GUI のスクリーンショットと成果物の検証: [docs/verification.md](docs/verification.md)。

## アニメーションのショーケース

[ショーケース一覧](examples/showcase.html) · [インタラクティブ SVG、直接イベント、不可視ステート](docs/interactive-svg.md)

- **[Sand / Accord](examples/desert-stickfight.html)** ([編集可能な `.sply`](examples/desert-stickfight.sply)): Cactus Runner の温かな砂漠スタイルで描いた 2 体のボーンキャラクターによる 20 秒のオリジナル短編。5 つの戦闘フェーズと友好的な結末。再生/一時停止/リプレイ。[プロジェクト、生成、Firefox での実地確認](docs/desert-stickfight.md)。
- [Cactus Runner](examples/cactus-runner.html): 走るボーンキャラクター、流れるサボテン、クリックでのジャンプ。歩き/ジャンプのウェイトを実際にブレンドします。[詳細/確認](docs/cactus-runner.md)。
- [Cinder / Flux – Stickfight](examples/stickfight.html): 2 体のボーンリグによる 8 秒間のオリジナル格闘アニメーション。回避/反撃、影/火花、再生/一時停止/再開。`python3 scripts/generate-stickfight.py`、確認: `python3 scripts/stickfight-smoke.py`。
- [茶色のフクロウ](examples/brown-owl.html): 枝にとまるフクロウ。クリック/ホバーで直接ステートが変わり、ホバーすると目がポインターを追って羽角が立ち、クリックすると満足げに目を閉じて翼を上げます。確認: `python3 scripts/owl-smoke.py`。
- [Aoi のあいさつ](examples/anime-hello.html): ボーンの手を持つオリジナルのアニメ風キャラクター。**ローカルで生成した WAV を SVG に埋め込み**、インターネット接続は不要です。クリックでオーディオが有効になります。再現: `python3 scripts/generate-anime-demo.py` (ローカルの `espeak-ng`/`espeak` が必要)、ブラウザーでの確認: `python3 scripts/anime-smoke.py`。
- [青い球に落ちる水滴](examples/water-drop.html): 伸縮・ハイライト・波紋を持つ自己完結した SVG シーン。再生/一時停止。
- [ランダムなフィギュア](examples/random-figures.html): シードで再現可能、キャプション付きでステートが自動で進みます。**Play** をクリック。ステートはクリップの終了に合わせて変わります。
- どちらの HTML も隣の SVG を `object` で埋め込み、サーバー API は使いません。テストサーバーはローカルのブラウザーテスト用の補助にすぎません。
- 生成: `splinely-cli water-drop --help`、`splinely-cli random-figures --help`。完全な例は [CLI.md](CLI.md)。ブラウザーでの確認: `python3 scripts/showcase-smoke.py --duration 35`。

## ロゴ

オリジナルの SVG ロゴ: アンカー、ハンドル、動きの軌跡を持つベジェスプラインで描いた S。

## ライセンス

MIT。[LICENSE](LICENSE) を参照してください。
