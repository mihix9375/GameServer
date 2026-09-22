# GameServer

GameLauncherへゲーム、更新、コメント、ランキングを提供する部内LAN向けサーバーです。gRPCサービスに加え、ブラウザーから運用できる管理画面とランキングHTTP APIを内蔵しています。

## 主な機能

- ゲーム一覧とバージョン情報の配信
- ゲームZIPのアップロード、更新、情報変更、削除
- SHA-256マニフェストによるファイル単位の差分配信
- 2 MiBチャンクでのストリーミング
- 配布ZIPの並列展開と安全なパス検証
- Launcherへの更新通知
- ゲーム別コメントの保存・管理
- ゲームごとに最大2つのランキングを管理
- Server PCまたは部内LANから使える管理画面

## ポート

| ポート | プロトコル | 用途 | 既定の待受 |
|---:|---|---|---|
| 50050 | gRPC | Launcher向けゲーム・コメント・更新API | IPv4/IPv6の全インターフェース |
| 50051 | HTTP | 管理画面 | `127.0.0.1`のみ |
| 50052 | HTTP | Launcher向けランキングAPI | IPv4の全インターフェース |

UnityゲームはGameServerへ直接接続しません。通信経路は `Unity → Launcher（127.0.0.1:50053）→ GameServer（50052）` です。

## すぐに起動する

配布された `Server.exe` を専用フォルダーへ置いて実行します。初回起動時に、実行ファイルと同じ場所へ必要なフォルダーと `admin-config.json` が作成されます。

管理画面はServer PCのブラウザーで次を開きます。

```text
http://127.0.0.1:50051
```

管理画面からゲームZIPの登録、ゲーム情報の変更、削除、更新通知、コメント管理、ランキング設定を行えます。

## ゲームZIPの作成

ZIPのルートに `meta.json` とゲーム本体を配置する構成を推奨します。

```text
SampleGame.zip
├─ meta.json
├─ SampleGame.exe
├─ SampleGame_Data/
└─ title.png
```

`meta.json` の例：

```json
{
  "id": "SampleGame",
  "title": "サンプルゲーム",
  "author": "Game Club",
  "titleImage": "title.png",
  "tags": ["Action", "Unity"],
  "game": "SampleGame.exe",
  "version": "0.1.0",
  "latestUpdate": "2026-09-22",
  "description": "ゲームの説明"
}
```

- `id`はゲームを識別する固定値です。更新時も変更しないでください。
- `game`はZIP内の実行ファイルへの相対パスです。
- `version`は `1.2.0` のように数字をピリオドで区切ります。先頭の `v` も受け付けます。
- 同じ`id`の新しいZIPをアップロードするとゲーム本体を更新できます。
- ZIPは20 GB以内にしてください。
- パストラバーサル、絶対パス、シンボリックリンク、重複パスを含むZIPは拒否されます。

## 管理画面のLAN公開

既定の `admin-config.json` は次の内容です。

```json
{
  "bind": "127.0.0.1:50051",
  "token": "",
  "leaderboard_bind": "0.0.0.0:50052"
}
```

部内LANから管理画面へ接続する場合はServerを停止し、次のように変更して再起動します。

```json
{
  "bind": "0.0.0.0:50051",
  "token": "推測されにくい長い管理トークン",
  "leaderboard_bind": "0.0.0.0:50052"
}
```

LAN公開時は管理トークンが必須です。空のままでは安全のため管理画面が起動しません。必要に応じてWindows FirewallでTCP 50050～50052の受信を許可してください。インターネットへの直接公開は想定していません。

詳しくは [ADMIN_UI.md](ADMIN_UI.md) を参照してください。

## 保存データ

データは `Server.exe` と同じフォルダーを基準に保存されます。

| パス | 内容 |
|---|---|
| `admin-config.json` | 管理画面とランキングAPIの設定 |
| `games/` | 配布ゲーム、ZIP、`games.json` |
| `temp/` | 取り込み待ちZIP |
| `comments.jsonl` | コメント |
| `leaderboards.json` | ランキング定義とスコア |

バックアップするときはServerを停止し、これらの設定・データをまとめてコピーしてください。

## 差分更新

Serverは配布ZIPからファイルサイズとSHA-256を含むマニフェストを生成し、ZIPが変更されるまでキャッシュします。Launcherはマニフェストを比較して必要なファイルだけを取得します。従来の完全ZIP配信も初回インストールと大規模更新用に維持しています。

詳細は [DIFFERENTIAL_UPDATES.md](DIFFERENTIAL_UPDATES.md) を参照してください。

## UnityランキングAPI

ゲームごとに最大2つ、`high_score`または`low_score`のランキングを設定できます。Unity向けエンドポイント、JSON形式、C#実装例は [UNITY_LEADERBOARD_API.md](UNITY_LEADERBOARD_API.md) を参照してください。

## ソースから実行する

必要なもの：

- Windows 10/11
- Rust stable（MSVC toolchain）
- Microsoft C++ Build Tools

protoはGit submoduleです。

```powershell
git clone --recursive https://github.com/mihix9375/GameServer.git
cd GameServer
cargo run --release
```

submoduleなしでcloneした場合は次を実行します。

```powershell
git submodule update --init --recursive
```

`cargo run`時のデータはビルドされた実行ファイルに隣接する `target\release` または `target\debug` 以下へ作成されます。

## ビルドとテスト

```powershell
cargo build --release
cargo test
```

実行ファイルは `target\release\Server.exe` に生成されます。

## protoの更新

共有定義は `proto` submoduleで管理しています。protoを変更するときは共有protoリポジトリへ先にpushし、このリポジトリではsubmodule参照を更新してコミットしてください。GameLauncher側では同じ共有protoをpullして使用します。

## 関連リポジトリ

- [GameLauncher](https://github.com/mihix9375/GameLauncher)
- [共有proto](https://github.com/mihix9375/proto)
