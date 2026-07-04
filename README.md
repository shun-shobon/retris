# retris

Game Boy Advance向けのテトリス。[テトリスガイドライン](docs/TETRIS_SPEC.md)に準拠し、SRS(スーパーローテーションシステム)と7バッグランダマイザーを実装する。Rust + [agb](https://agbrs.dev) 0.24.0製。

## リポジトリ構成

| パス | 内容 |
| --- | --- |
| `core/` | `retris-core` — `no_std`のゲームロジック。ホスト上で`cargo test`可能 |
| `gba/` | `retris` — agb依存のGBAバイナリ(ターゲット: `thumbv4t-none-eabi`) |
| `docs/TETRIS_SPEC.md` | 実装仕様書 |
| `tools/` | `agb-gbafix`・mGBAなどの開発ツール(git管理外) |

## ビルドと実行

### ゲームロジックのテスト

```sh
cd core
cargo test
```

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

起動に成功すると mGBA のログに `retris boot ok` が出力される。

## Delta Emulator (iOS) への導入

1. 生成した `retris.gba` を AirDrop・iCloud Drive などでiOSデバイスのFilesアプリに転送する。
2. Deltaを開き、`+` → 「Files」から `retris.gba` を選択してインポートする。
