using System;
using System.Collections;
using UnityEngine;
using UnityEngine.Networking;

public sealed class UnityLeaderboardClient : MonoBehaviour
{
    private const string LauncherApi = "http://127.0.0.1:50053";
	private static string SessionToken => Environment.GetEnvironmentVariable("GAMELAUNCHER_SESSION_TOKEN");

    [Serializable] public sealed class ScoreEntry
    {
        public int rank;
        public string player_name;
        public long score;
        public long submitted_at;
    }

    [Serializable] public sealed class Leaderboard
    {
        public string id;
        public string name;
        public string order;
        public ScoreEntry[] entries;
    }

    [Serializable] public sealed class LeaderboardResponse
    {
        public Leaderboard[] leaderboards;
    }

    [Serializable] private sealed class ScoreRequest
    {
        public string player_name;
        public long score;
    }

    [Serializable] public sealed class ScoreResponse
    {
        public bool ok;
        public int rank;
    }

    public IEnumerator GetLeaderboards(Action<LeaderboardResponse> onSuccess, Action<string> onError)
    {
        string url = $"{LauncherApi}/v1/leaderboards";
        using var request = UnityWebRequest.Get(url);
		request.SetRequestHeader("Authorization", $"Bearer {SessionToken}");
        request.timeout = 5;
        yield return request.SendWebRequest();
        if (request.result != UnityWebRequest.Result.Success)
        {
            onError?.Invoke(request.downloadHandler.text.Length > 0 ? request.downloadHandler.text : request.error);
            yield break;
        }
        onSuccess?.Invoke(JsonUtility.FromJson<LeaderboardResponse>(request.downloadHandler.text));
    }

    public IEnumerator SubmitScore(int slot, string playerName, long score, Action<ScoreResponse> onSuccess, Action<string> onError)
    {
        if (slot < 0 || slot > 1) throw new ArgumentOutOfRangeException(nameof(slot));
        string url = $"{LauncherApi}/v1/leaderboards/{slot}/scores";
        string json = JsonUtility.ToJson(new ScoreRequest { player_name = playerName, score = score });
        using var request = new UnityWebRequest(url, UnityWebRequest.kHttpVerbPOST);
        request.uploadHandler = new UploadHandlerRaw(System.Text.Encoding.UTF8.GetBytes(json));
        request.downloadHandler = new DownloadHandlerBuffer();
        request.SetRequestHeader("Content-Type", "application/json");
		request.SetRequestHeader("Authorization", $"Bearer {SessionToken}");
        request.timeout = 5;
        yield return request.SendWebRequest();
        if (request.result != UnityWebRequest.Result.Success)
        {
            onError?.Invoke(request.downloadHandler.text.Length > 0 ? request.downloadHandler.text : request.error);
            yield break;
        }
        onSuccess?.Invoke(JsonUtility.FromJson<ScoreResponse>(request.downloadHandler.text));
    }
}
