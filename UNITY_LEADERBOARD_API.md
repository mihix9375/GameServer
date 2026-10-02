# UnityランキングAPI

通信経路は `Unityゲーム → GameLauncher → GameServer` です。ゲームはServerのIPアドレスを保持せず、同じPCで起動中のLauncherだけに接続します。

## 前提

1. 必要ならGameServer管理画面でランキングを設定します。ゲーム側の配列から同期することもできます。
2. Launcherの設定で「ランキングServer API」に `http://ServerのIP:50052` を指定します。
3. Launcherを起動した状態でゲームを起動します。

LauncherがUnity向けAPIを `http://127.0.0.1:50053` で提供します。Launcher終了時にはAPIも終了します。

Launcherは起動ごとのセッショントークンから実際に起動中のゲームを特定します。ゲームIDをURLやリクエストへ指定する方法はありません。通常はUnity Packageを使用してください。

## 一覧を取得

```http
GET http://127.0.0.1:50053/v1/leaderboards
Authorization: Bearer {GameLauncherが起動時に渡すトークン}
```

上位10件、最大2ランキングを返します。

```json
{
  "leaderboards": [
    {
      "id": "0",
      "name": "ハイスコア",
      "order": "high_score",
      "enabled": true,
      "entries": [
        { "rank": 1, "player_name": "PLAYER", "score": 12000, "submitted_at": 1789980000 }
      ]
    }
  ]
}
```

`order` は `high_score`（大きい値が上）または `low_score`（小さい値が上）です。

## ランキング設定を同期

```http
PUT http://127.0.0.1:50053/v1/leaderboards
Authorization: Bearer {GameLauncherが起動時に渡すトークン}
Content-Type: application/json

{"leaderboards":[{"name":"ハイスコア","order":"high_score","enabled":true},{"name":"ランキング2","order":"high_score","enabled":false}]}
```

配列の0番がランキング0、1番がランキング1です。`enabled`で表示とスコア受付を切り替えます。無効化しても既存の内部IDとスコアは同じ位置に維持されます。

## スコアを投稿

```http
POST http://127.0.0.1:50053/v1/leaderboards/{slot}/scores
Authorization: Bearer {GameLauncherが起動時に渡すトークン}
Content-Type: application/json

{"player_name":"PLAYER","score":12000}
```

成功時は保存後の順位を返します。

```json
{ "ok": true, "rank": 1 }
```

- プレイヤー名は1〜24文字です。
- `slot`は`0`または`1`です。
- スコアは符号付き64bit整数です。タイムランキングではミリ秒など、ゲーム内で単位を統一してください。
- Serverは各ランキングの上位100件を保存します。同点の場合は先に投稿された記録が上になります。

Unity用の実装例は [`examples/UnityLeaderboardClient.cs`](examples/UnityLeaderboardClient.cs) にあります。
