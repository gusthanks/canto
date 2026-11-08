# SimplifiedTrackObject

## Properties

Name | Type | Description | Notes
------------ | ------------- | ------------- | -------------
**artists** | Option<[**Vec<models::SimplifiedArtistObject>**](SimplifiedArtistObject.md)> | The artists who performed the track. Each artist object includes a link in `href` to more detailed information about the artist. | [optional]
**available_markets** | Option<**Vec<String>**> | A list of the countries in which the track can be played, identified by their [ISO 3166-1 alpha-2](http://en.wikipedia.org/wiki/ISO_3166-1_alpha-2) code.  | [optional]
**disc_number** | Option<**i32**> | The disc number (usually `1` unless the album consists of more than one disc). | [optional]
**duration_ms** | Option<**i32**> | The track length in milliseconds. | [optional]
**explicit** | Option<**bool**> | Whether or not the track has explicit lyrics ( `true` = yes it does; `false` = no it does not OR unknown). | [optional]
**external_urls** | Option<[**models::ExternalUrlObject**](ExternalUrlObject.md)> | External URLs for this track.  | [optional]
**href** | Option<**String**> | A link to the Web API endpoint providing full details of the track. | [optional]
**id** | Option<**String**> | The [Spotify ID](/documentation/web-api/concepts/spotify-uris-ids) for the track.  | [optional]
**is_playable** | Option<**bool**> | Part of the response when [Track Relinking](/documentation/web-api/concepts/track-relinking/) is applied. If `true`, the track is playable in the given market. Otherwise `false`.  | [optional]
**linked_from** | Option<[**models::LinkedTrackObject**](LinkedTrackObject.md)> | Part of the response when [Track Relinking](/documentation/web-api/concepts/track-relinking/) is applied and is only part of the response if the track linking, in fact, exists. The requested track has been replaced with a different track. The track in the `linked_from` object contains information about the originally requested track. | [optional]
**restrictions** | Option<[**models::TrackRestrictionObject**](TrackRestrictionObject.md)> | Included in the response when a content restriction is applied.  | [optional]
**name** | Option<**String**> | The name of the track. | [optional]
**preview_url** | Option<**String**> | A URL to a 30 second preview (MP3 format) of the track.  | [optional]
**track_number** | Option<**i32**> | The number of the track. If an album has several discs, the track number is the number on the specified disc.  | [optional]
**r#type** | Option<**String**> | The object type: \"track\".  | [optional]
**uri** | Option<**String**> | The [Spotify URI](/documentation/web-api/concepts/spotify-uris-ids) for the track.  | [optional]
**is_local** | Option<**bool**> | Whether or not the track is from a local file.  | [optional]

[[Back to Model list]](../README.md#documentation-for-models) [[Back to API list]](../README.md#documentation-for-api-endpoints) [[Back to README]](../README.md)


