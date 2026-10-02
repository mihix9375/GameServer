# GameServer

GameLauncherへゲーム、更新、コメント、ランキングを提供する部内LAN向けサーバーです。gRPCサービスに加え、ブラウザーから運用できる管理画面とランキングHTTP APIを内蔵しています。

## 主な機能

- ゲーム一覧とバージョン情報の配信
- ゲームZIPのアップロード、更新、情報変更、削除
- 配布終了したゲームをLauncherから削除する通知とオフライン同期
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

## 全体構成

```text
Unityゲーム
  └─ GameLauncher-Unity-Ranking（UPMパッケージ）
       └─ GameLauncher :50053
            ├─ GameServer :50050（gRPC）
            └─ GameServer :50052（ランキングHTTP API）

運用ブラウザー
  └─ GameServer :50051（管理画面）
```

ゲーム側のランキングAPIにはゲームIDやServerのIPアドレスを渡しません。Launcherが起動セッションから対象ゲームを安全に特定します。

## すぐに起動する

配布された `Server.exe` を専用フォルダーへ置いて実行します。初回起動時に、実行ファイルと同じ場所へ必要なフォルダーと `admin-config.json` が作成されます。

管理画面はServer PCのブラウザーで次を開きます。

```text
http://127.0.0.1:50051
```

管理画面からゲームZIPの登録、ゲーム情報の変更、削除、更新通知、コメント管理、ランキング設定、接続中Launcherの通信状況確認を行えます。

配布ゲーム一覧の「管理・編集」を開くと、詳細・ランキング・コメント・ZIP再アップロードを1つの画面で切り替えて操作できます。ZIP再アップロードは、選択中ゲームとZIP内 `meta.json` のIDが一致する場合だけ受け付けます。

## ログ

コンソールの運用ログには、Server PCのローカル時刻（タイムゾーンとミリ秒を含む）が表示されます。

通信の詳細は `logs/server-details.jsonl` にJSON Lines形式で追記され、コンソールには表示されません。各行には時刻、RPC名、接続元IP、要求・応答サイズ、成功またはgRPCエラーが記録されます。コメント本文やゲームデータ本体は記録しません。

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
- ZIPやフォルダーの名前にかかわらず、空でない`id`がServer上のゲームIDになります。`id`が空の場合だけZIP名を代用します。
- `game`はZIP内の実行ファイルへの相対パスです。
- `titleImage`にローカル画像を指定する場合は、`meta.json`からの相対パスに画像を含めてください。存在しない画像を参照するZIPはアップロード時に拒否されます。
- `version`は `1.2.0` のように数字をピリオドで区切ります。先頭の `v` も受け付けます。
- `latestUpdate`は`YYYY/MM/DD`で表示されます。`YYYY-MM-DD`、`YYYY.M.D`、`YYYY年M月D日`、`YYYYMMDD`、ISO 8601日時も受け付けます。
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
| `removed-games.json` | オフラインLauncherへ同期するゲーム削除履歴 |
| `logs/server-details.jsonl` | コンソールに出さない通信詳細ログ |

バックアップするときはServerを停止し、これらの設定・データをまとめてコピーしてください。

## 差分更新

Serverは配布ZIPからファイルサイズとSHA-256を含むマニフェストを生成し、ZIPが変更されるまでキャッシュします。Launcherはマニフェストを比較して必要なファイルだけを取得します。従来の完全ZIP配信も初回インストールと大規模更新用に維持しています。

管理画面からゲームを削除すると、接続中のLauncherへ即時に削除通知を送ります。削除履歴は `removed-games.json` に保存され、オフラインだったLauncherにも次回接続時に反映されます。別Serverへ接続しただけではローカルゲームを削除しません。

詳細は [DIFFERENTIAL_UPDATES.md](DIFFERENTIAL_UPDATES.md) を参照してください。

## UnityランキングAPI

ゲームごとに最大2つ、`high_score`（大きい値が上位）または`low_score`（小さい値が上位）のランキングを設定できます。

> [!IMPORTANT]
> ランキングAPI v0.2.0は旧APIと後方互換性がありません。ゲーム側は`game_id`や自由文字列のランキングIDを送らず、GameLauncherが発行するセッショントークンと固定スロット`0`・`1`を使用します。GameServer v0.5.0、GameLauncher v0.6.0、UPMパッケージ v0.2.0を組み合わせてください。

1. Unityへ次のGit URLからパッケージを追加します。

```text
https://github.com/mihix9375/GameLauncher-Unity-Ranking.git#v0.2.0
```

2. ゲームの初期化時などにランキングを作成します。

```csharp
var ranks = await RankingApi.SyncLeaderboardsAsync();
await ranks[0].SetAsync("ハイスコア", RankingOrder.HighScore);
await ranks[0].EnableAsync();
```

3. ゲームクリア時などにスコア送信関数を呼び出します。

```csharp
ScoreResult result = await ranks[0].InsertAsync(playerName, score);
```

Unity側の導入方法は [GameLauncher-Unity-Ranking](https://github.com/mihix9375/GameLauncher-Unity-Ranking) を参照してください。HTTPエンドポイントとJSON形式は [UNITY_LEADERBOARD_API.md](UNITY_LEADERBOARD_API.md) に記載しています。

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
- [GameLauncher Ranking API for Unity](https://github.com/mihix9375/GameLauncher-Unity-Ranking)
- [共有proto](https://github.com/mihix9375/proto)
