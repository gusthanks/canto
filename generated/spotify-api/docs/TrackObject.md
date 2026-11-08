# TrackObject

## Properties

Name | Type | Description | Notes
------------ | ------------- | ------------- | -------------
**album** | Option<[**models::SimplifiedAlbumObject**](SimplifiedAlbumObject.md)> | The album on which the track appears. The album object includes a link in `href` to full information about the album.  | [optional]
**artists** | Option<[**Vec<models::SimplifiedArtistObject>**](SimplifiedArtistObject.md)> | The artists who performed the track. Each artist object includes a link in `href` to more detailed information about the artist.  | [optional]
**available_markets** | Option<**Vec<String>**> | A list of the countries in which the track can be played, identified by their [ISO 3166-1 alpha-2](http://en.wikipedia.org/wiki/ISO_3166-1_alpha-2) code.  | [optional]
**disc_number** | Option<**i32**> | The disc number (usually `1` unless the album consists of more than one disc).  | [optional]
**duration_ms** | Option<**i32**> | The track length in milliseconds.  | [optional]
**explicit** | Option<**bool**> | Whether or not the track has explicit lyrics ( `true` = yes it does; `false` = no it does not OR unknown).  | [optional]
**external_ids** | Option<[**models::ExternalIdObject**](ExternalIdObject.md)> | Known external IDs for the track.  | [optional]
**external_urls** | Option<[**models::ExternalUrlObject**](ExternalUrlObject.md)> | Known external URLs for this track.  | [optional]
**href** | Option<**String**> | A link to the Web API endpoint providing full details of the track.  | [optional]
**id** | Option<**String**> | The [Spotify ID](/documentation/web-api/concepts/spotify-uris-ids) for the track.  | [optional]
**is_playable** | Option<**bool**> | Part of the response when [Track Relinking](/documentation/web-api/concepts/track-relinking) is applied. If `true`, the track is playable in the given market. Otherwise `false`.  | [optional]
**linked_from** | Option<[**serde_json::Value**](.md)> | Part of the response when [Track Relinking](/documentation/web-api/concepts/track-relinking) is applied, and the requested track has been replaced with different track. The track in the `linked_from` object contains information about the originally requested track.  | [optional]
**restrictions** | Option<[**models::TrackRestrictionObject**](TrackRestrictionObject.md)> | Included in the response when a content restriction is applied.  | [optional]
**name** | Option<**String**> | The name of the track.  | [optional]
**popularity** | Option<**i32**> | The popularity of the track. The value will be between 0 and 100, with 100 being the most popular.<br/>The popularity of a track is a value between 0 and 100, with 100 being the most popular. The popularity is calculated by algorithm and is based, in the most part, on the total number of plays the track has had and how recent those plays are.<br/>Generally speaking, songs that are being played a lot now will have a higher popularity than songs that were played a lot in the past. Duplicate tracks (e.g. the same track from a single and an album) are rated independently. Artist and album popularity is derived mathematically from track popularity. _**Note**: the popularity value may lag actual popularity by a few days: the value is not updated in real time._  | [optional]
**preview_url** | Option<**String**> | A link to a 30 second preview (MP3 format) of the track. Can be `null`  | [optional]
**track_number** | Option<**i32**> | The number of the track. If an album has several discs, the track number is the number on the specified disc.  | [optional]
**r#type** | Option<**String**> | The object type: \"track\".  | [optional]
**uri** | Option<**String**> | The [Spotify URI](/documentation/web-api/concepts/spotify-uris-ids) for the track.  | [optional]
**is_local** | Option<**bool**> | Whether or not the track is from a local file.  | [optional]

[[Back to Model list]](../README.md#documentation-for-models) [[Back to API list]](../README.md#documentation-for-api-endpoints) [[Back to README]](../README.md)


