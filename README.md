# retris

Game Boy Advance向けのテトリス。[テトリスガイドライン](docs/TETRIS_SPEC.md)に準拠し、Rust + [agb](https://agbrs.dev) 0.24.0で実装している。

実装済みの主な機能:

- ガイドライン準拠のゲームプレイ: SRS(スーパーローテーションシステム)・7バッグランダマイザー・ホールド・ネクスト5個表示・ゴーストピース・T-Spin判定・Back-to-Back・コンボ・Perfect Clear・ロックディレイ(move reset)・DAS/ARR
- 開始レベル1〜15を選択可能(重力はレベル20相当で頭打ち)
- タイトル・ポーズ・ゲームオーバー画面
- 効果音とBGM(コロブチカ)。すべてスクリプトで自前合成

## 操作方法

| ボタン | 操作 |
| --- | --- |
| 十字キー 左/右 | 左右移動 |
| 十字キー 下 | ソフトドロップ |
| 十字キー 上 | ハードドロップ |
| A | 右回転 |
| B | 左回転 |
| L / R | ホールド |
| START | ゲーム開始 / ポーズ |

## リポジトリ構成

| パス | 内容 |
| --- | --- |
| `core/` | `retris-core` — `no_std`のゲームロジック。ホスト上で`cargo test`可能 |
| `gba/` | `retris` — agb依存のGBAバイナリ(ターゲット: `thumbv4t-none-eabi`) |
| `gba/assets-gen/` | 効果音・BGMを合成する生成スクリプト |
| `gba/sfx/` | 生成済みのサウンドアセット(wav、コミット済み) |
| `docs/TETRIS_SPEC.md` | 実装仕様書 |
| `tools/` | `agb-gbafix`・mGBAなどの開発ツール(git管理外) |

## ビルドと実行

### ゲームロジックのテスト

```sh
cd core
cargo test
```

ユニットテスト181個とランダム入力プレイスルーテスト3個の計184テストが実行される。

### GBAバイナリのビルド

```sh
cd gba
cargo build --release
```

### ROM(.gba)の生成

```sh
tools/bin/agb-gbafix gba/target/thumbv4t-none-eabi/release/retris -o retris.gba
```

### mGBAでの実行

```sh
cd gba
cargo run --release
```

ゲームを開始すると mGBA のログに `retris: start (seed=..., level=...)` が出力される。

### サウンドアセットの再生成

`python3 gba/assets-gen/gen_sfx.py` で `gba/sfx/*.wav` を決定的に再生成できる(wavはコミット済みのため通常は不要)。

## Delta Emulator (iOS) への導入

1. 生成した `retris.gba` を AirDrop・iCloud Drive などでiOSデバイスのFilesアプリに転送する。
2. Deltaを開き、`+` → 「Files」から `retris.gba` を選択してインポートする。
