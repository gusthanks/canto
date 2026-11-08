# SimplifiedEpisodeObject

## Properties

Name | Type | Description | Notes
------------ | ------------- | ------------- | -------------
**audio_preview_url** | Option<**String**> | A URL to a 30 second preview (MP3 format) of the episode. `null` if not available.  | 
**description** | **String** | A description of the episode. HTML tags are stripped away from this field, use `html_description` field in case HTML tags are needed.  | 
**html_description** | **String** | A description of the episode. This field may contain HTML tags.  | 
**duration_ms** | **i32** | The episode length in milliseconds.  | 
**explicit** | **bool** | Whether or not the episode has explicit content (true = yes it does; false = no it does not OR unknown).  | 
**external_urls** | [**models::ExternalUrlObject**](ExternalUrlObject.md) | External URLs for this episode.  | 
**href** | **String** | A link to the Web API endpoint providing full details of the episode.  | 
**id** | **String** | The [Spotify ID](/documentation/web-api/concepts/spotify-uris-ids) for the episode.  | 
**images** | [**Vec<models::ImageObject>**](ImageObject.md) | The cover art for the episode in various sizes, widest first.  | 
**is_externally_hosted** | **bool** | True if the episode is hosted outside of Spotify's CDN.  | 
**is_playable** | **bool** | True if the episode is playable in the given market. Otherwise false.  | 
**language** | Option<**String**> | The language used in the episode, identified by a [ISO 639](https://en.wikipedia.org/wiki/ISO_639) code. This field is deprecated and might be removed in the future. Please use the `languages` field instead.  | [optional]
**languages** | **Vec<String>** | A list of the languages used in the episode, identified by their [ISO 639-1](https://en.wikipedia.org/wiki/ISO_639) code.  | 
**name** | **String** | The name of the episode.  | 
**release_date** | **String** | The date the episode was first released, for example `\"1981-12-15\"`. Depending on the precision, it might be shown as `\"1981\"` or `\"1981-12\"`.  | 
**release_date_precision** | **String** | The precision with which `release_date` value is known.  | 
**resume_point** | Option<[**models::ResumePointObject**](ResumePointObject.md)> | The user's most recent position in the episode. Set if the supplied access token is a user token and has the scope 'user-read-playback-position'.  | [optional]
**r#type** | **String** | The object type.  | 
**uri** | **String** | The [Spotify URI](/documentation/web-api/concepts/spotify-uris-ids) for the episode.  | 
**restrictions** | Option<[**models::EpisodeRestrictionObject**](EpisodeRestrictionObject.md)> | Included in the response when a content restriction is applied.  | [optional]

[[Back to Model list]](../README.md#documentation-for-models) [[Back to API list]](../README.md#documentation-for-api-endpoints) [[Back to README]](../README.md)


