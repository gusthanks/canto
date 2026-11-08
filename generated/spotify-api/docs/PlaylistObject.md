# PlaylistObject

## Properties

Name | Type | Description | Notes
------------ | ------------- | ------------- | -------------
**collaborative** | Option<**bool**> | `true` if the owner allows other users to modify the playlist.  | [optional]
**description** | Option<**String**> | The playlist description. _Only returned for modified, verified playlists, otherwise_ `null`.  | [optional]
**external_urls** | Option<[**models::ExternalUrlObject**](ExternalUrlObject.md)> | Known external URLs for this playlist.  | [optional]
**href** | Option<**String**> | A link to the Web API endpoint providing full details of the playlist.  | [optional]
**id** | Option<**String**> | The [Spotify ID](/documentation/web-api/concepts/spotify-uris-ids) for the playlist.  | [optional]
**images** | Option<[**Vec<models::ImageObject>**](ImageObject.md)> | Images for the playlist. The array may be empty or contain up to three images. The images are returned by size in descending order. See [Working with Playlists](/documentation/web-api/concepts/playlists). _**Note**: If returned, the source URL for the image (`url`) is temporary and will expire in less than a day._  | [optional]
**name** | Option<**String**> | The name of the playlist.  | [optional]
**owner** | Option<[**models::PlaylistOwnerObject**](PlaylistOwnerObject.md)> | The user who owns the playlist  | [optional]
**public** | Option<**bool**> | The playlist's public/private status (if it is added to the user's profile): `true` the playlist is public, `false` the playlist is private, `null` the playlist status is not relevant. For more about public/private status, see [Working with Playlists](/documentation/web-api/concepts/playlists)  | [optional]
**snapshot_id** | Option<**String**> | The version identifier for the current playlist. Can be supplied in other requests to target a specific playlist version  | [optional]
**items** | Option<[**models::PagingPlaylistTrackObject**](PagingPlaylistTrackObject.md)> | The items of the playlist. _**Note**: This field is only available for playlists owned by the current user or playlists the user is a collaborator of._  | [optional]
**tracks** | Option<[**models::PagingPlaylistTrackObject**](PagingPlaylistTrackObject.md)> | **Deprecated:** Use `items` instead. The tracks of the playlist.  | [optional]
**r#type** | Option<**String**> | The object type: \"playlist\"  | [optional]
**uri** | Option<**String**> | The [Spotify URI](/documentation/web-api/concepts/spotify-uris-ids) for the playlist.  | [optional]

[[Back to Model list]](../README.md#documentation-for-models) [[Back to API list]](../README.md#documentation-for-api-endpoints) [[Back to README]](../README.md)


