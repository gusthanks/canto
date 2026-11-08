# ArtistDiscographyAlbumObject

## Properties

Name | Type | Description | Notes
------------ | ------------- | ------------- | -------------
**album_type** | Option<**String**> | The type of the album.  | 
**total_tracks** | Option<**i32**> | The number of tracks in the album. | [optional]
**available_markets** | Option<**Vec<String>**> | The markets in which the album is available: [ISO 3166-1 alpha-2 country codes](http://en.wikipedia.org/wiki/ISO_3166-1_alpha-2). _**NOTE**: an album is considered available in a market when at least 1 of its tracks is available in that market._  | [optional]
**external_urls** | [**models::ExternalUrlObject**](ExternalUrlObject.md) | Known external URLs for this album.  | 
**href** | Option<**String**> | A link to the Web API endpoint providing full details of the album.  | 
**id** | Option<**String**> | The [Spotify ID](/documentation/web-api/concepts/spotify-uris-ids) for the album.  | 
**images** | [**Vec<models::ImageObject>**](ImageObject.md) | The cover art for the album in various sizes, widest first.  | 
**name** | **String** | The name of the album. In case of an album takedown, the value may be an empty string.  | 
**release_date** | Option<**String**> | The date the album was first released.  | 
**release_date_precision** | Option<**String**> | The precision with which `release_date` value is known.  | 
**restrictions** | Option<[**models::AlbumRestrictionObject**](AlbumRestrictionObject.md)> | Included in the response when a content restriction is applied.  | [optional]
**r#type** | **String** | The object type.  | 
**uri** | Option<**String**> | The [Spotify URI](/documentation/web-api/concepts/spotify-uris-ids) for the album.  | 
**artists** | [**Vec<models::SimplifiedArtistObject>**](SimplifiedArtistObject.md) | The artists of the album. Each artist object includes a link in `href` to more detailed information about the artist.  | 
**album_group** | **String** | This field describes the relationship between the artist and the album.  | 

[[Back to Model list]](../README.md#documentation-for-models) [[Back to API list]](../README.md#documentation-for-api-endpoints) [[Back to README]](../README.md)


