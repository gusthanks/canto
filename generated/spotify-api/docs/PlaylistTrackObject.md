# PlaylistTrackObject

## Properties

Name | Type | Description | Notes
------------ | ------------- | ------------- | -------------
**added_at** | Option<**String**> | The date and time the track or episode was added. _**Note**: some very old playlists may return `null` in this field._  | [optional]
**added_by** | Option<[**models::PlaylistUserObject**](PlaylistUserObject.md)> | The Spotify user who added the track or episode. _**Note**: some very old playlists may return `null` in this field._  | [optional]
**is_local** | Option<**bool**> | Whether this track or episode is a [local file](/documentation/web-api/concepts/playlists/#local-files) or not.  | [optional]
**item** | Option<[**models::PlaylistTrackObjectItem**](PlaylistTrackObject_item.md)> |  | [optional]
**track** | Option<[**models::PlaylistTrackObjectTrack**](PlaylistTrackObject_track.md)> |  | [optional]

[[Back to Model list]](../README.md#documentation-for-models) [[Back to API list]](../README.md#documentation-for-api-endpoints) [[Back to README]](../README.md)


