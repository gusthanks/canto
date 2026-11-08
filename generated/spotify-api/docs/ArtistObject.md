# ArtistObject

## Properties

Name | Type | Description | Notes
------------ | ------------- | ------------- | -------------
**external_urls** | Option<[**models::ExternalUrlObject**](ExternalUrlObject.md)> | Known external URLs for this artist.  | [optional]
**followers** | Option<[**models::FollowersObject**](FollowersObject.md)> | Information about the followers of the artist.  | [optional]
**genres** | Option<**Vec<String>**> | A list of the genres the artist is associated with. If not yet classified, the array is empty.  | [optional]
**href** | Option<**String**> | A link to the Web API endpoint providing full details of the artist.  | [optional]
**id** | Option<**String**> | The [Spotify ID](/documentation/web-api/concepts/spotify-uris-ids) for the artist.  | [optional]
**images** | Option<[**Vec<models::ImageObject>**](ImageObject.md)> | Images of the artist in various sizes, widest first.  | [optional]
**name** | Option<**String**> | The name of the artist.  | [optional]
**popularity** | Option<**i32**> | The popularity of the artist. The value will be between 0 and 100, with 100 being the most popular. The artist's popularity is calculated from the popularity of all the artist's tracks.  | [optional]
**r#type** | Option<**String**> | The object type.  | [optional]
**uri** | Option<**String**> | The [Spotify URI](/documentation/web-api/concepts/spotify-uris-ids) for the artist.  | [optional]

[[Back to Model list]](../README.md#documentation-for-models) [[Back to API list]](../README.md#documentation-for-api-endpoints) [[Back to README]](../README.md)


