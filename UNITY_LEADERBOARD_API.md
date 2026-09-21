# UnityランキングAPI

通信経路は `Unityゲーム → GameLauncher → GameServer` です。ゲームはServerのIPアドレスを保持せず、同じPCで起動中のLauncherだけに接続します。

## 前提

1. GameServer管理画面の「配布ゲーム」から対象ゲームの「ランキング」を開き、最大2つのランキングを設定します。
2. Launcherの設定で「ランキングServer API」に `http://ServerのIP:50052` を指定します。
3. Launcherを起動した状態でゲームを起動します。

LauncherがUnity向けAPIを `http://127.0.0.1:50053` で提供します。Launcher終了時にはAPIも終了します。

## 一覧を取得

```http
GET http://127.0.0.1:50053/v1/games/{game_id}/leaderboards
```

上位10件、最大2ランキングを返します。

```json
{
  "game_id": "SampleGame",
  "leaderboards": [
    {
      "id": "score",
      "name": "ハイスコア",
      "order": "high_score",
      "entries": [
        { "rank": 1, "player_name": "PLAYER", "score": 12000, "submitted_at": 1789980000 }
      ]
    }
  ]
}
```

`order` は `high_score`（大きい値が上）または `low_score`（小さい値が上）です。

## スコアを投稿

```http
POST http://127.0.0.1:50053/v1/games/{game_id}/leaderboards/{leaderboard_id}/scores
Content-Type: application/json

{"player_name":"PLAYER","score":12000}
```

成功時は保存後の順位を返します。

```json
{ "ok": true, "rank": 1 }
```

- プレイヤー名は1〜24文字です。
- スコアは符号付き64bit整数です。タイムランキングではミリ秒など、ゲーム内で単位を統一してください。
- Serverは各ランキングの上位100件を保存します。同点の場合は先に投稿された記録が上になります。

Unity用の実装例は [`examples/UnityLeaderboardClient.cs`](examples/UnityLeaderboardClient.cs) にあります。
