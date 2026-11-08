# StartAUsersPlaybackRequest

## Properties

Name | Type | Description | Notes
------------ | ------------- | ------------- | -------------
**context_uri** | Option<**String**> | Optional. Spotify URI of the context to play. Valid contexts are albums, artists & playlists. `{context_uri:\"spotify:album:1Je1IMUlBXcx1Fz0WE7oPT\"}`  | [optional]
**uris** | Option<**Vec<String>**> | Optional. A JSON array of the Spotify track URIs to play. For example: `{\"uris\": [\"spotify:track:4iV5W9uYEdYUVa79Axb7Rh\", \"spotify:track:1301WleyT98MSxVHPZCA6M\"]}`  | [optional]
**offset** | Option<[**std::collections::HashMap<String, serde_json::Value>**](serde_json::Value.md)> | Optional. Indicates from where in the context playback should start. Only available when context_uri corresponds to an album or playlist object \"position\" is zero based and can’t be negative. Example: `\"offset\": {\"position\": 5}` \"uri\" is a string representing the uri of the item to start at. Example: `\"offset\": {\"uri\": \"spotify:track:1301WleyT98MSxVHPZCA6M\"}`  | [optional]
**position_ms** | Option<**i32**> | integer | [optional]

[[Back to Model list]](../README.md#documentation-for-models) [[Back to API list]](../README.md#documentation-for-api-endpoints) [[Back to README]](../README.md)


