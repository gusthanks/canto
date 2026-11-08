# SimplifiedShowObject

## Properties

Name | Type | Description | Notes
------------ | ------------- | ------------- | -------------
**available_markets** | **Vec<String>** | A list of the countries in which the show can be played, identified by their [ISO 3166-1 alpha-2](http://en.wikipedia.org/wiki/ISO_3166-1_alpha-2) code.  | 
**copyrights** | [**Vec<models::CopyrightObject>**](CopyrightObject.md) | The copyright statements of the show.  | 
**description** | **String** | A description of the show. HTML tags are stripped away from this field, use `html_description` field in case HTML tags are needed.  | 
**html_description** | **String** | A description of the show. This field may contain HTML tags.  | 
**explicit** | **bool** | Whether or not the show has explicit content (true = yes it does; false = no it does not OR unknown).  | 
**external_urls** | [**models::ExternalUrlObject**](ExternalUrlObject.md) | External URLs for this show.  | 
**href** | **String** | A link to the Web API endpoint providing full details of the show.  | 
**id** | **String** | The [Spotify ID](/documentation/web-api/concepts/spotify-uris-ids) for the show.  | 
**images** | [**Vec<models::ImageObject>**](ImageObject.md) | The cover art for the show in various sizes, widest first.  | 
**is_externally_hosted** | **bool** | True if all of the shows episodes are hosted outside of Spotify's CDN. This field might be `null` in some cases.  | 
**languages** | **Vec<String>** | A list of the languages used in the show, identified by their [ISO 639](https://en.wikipedia.org/wiki/ISO_639) code.  | 
**media_type** | **String** | The media type of the show.  | 
**name** | **String** | The name of the episode.  | 
**publisher** | **String** | The publisher of the show.  | 
**r#type** | **String** | The object type.  | 
**uri** | **String** | The [Spotify URI](/documentation/web-api/concepts/spotify-uris-ids) for the show.  | 
**total_episodes** | **i32** | The total number of episodes in the show.  | 

[[Back to Model list]](../README.md#documentation-for-models) [[Back to API list]](../README.md#documentation-for-api-endpoints) [[Back to README]](../README.md)


